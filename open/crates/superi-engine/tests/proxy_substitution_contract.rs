use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use superi_cache::key::MediaCacheIdentity;
use superi_cache::proxy::{
    DerivedMediaArtifact, DerivedMediaCatalog, DerivedMediaPurpose, DerivedMediaQuality,
    DerivedMediaRequest,
};
use superi_concurrency::jobs::{DerivedFallbackPolicy, DerivedQuality, DerivedSelectionReason};
use superi_core::color_space::ColorSpace;
use superi_core::error::{ErrorCategory, Result};
use superi_core::ids::MediaId;
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_core::time::{Duration, RationalTime, Timebase};
use superi_engine::derived_media::{
    derived_media_render_settings, generate_derived_media, EncodedDerivedMedia,
};
use superi_engine::media::media_backend_registry;
use superi_engine::proxy_substitution::{resolve_proxy_source, ProxySubstitutionRequest};
use superi_media_io::decode::{CpuVideoBuffer, VideoFormat, VideoFrame, VideoPlane};
use superi_media_io::demux::{
    CodecId, MediaSource, Packet, PacketTiming, SeekMode, SeekRequest, SourceIdentity, SourceInfo,
    StreamId, StreamInfo, StreamKind,
};
use superi_media_io::encode::{EncodeInput, EncoderConfig};
use superi_media_io::operation::{MediaPriority, OperationContext};
use superi_media_io::read::ReadOutcome;

const PROXY_STREAM: StreamId = StreamId::new(41);
const ORIGINAL_STREAM: StreamId = StreamId::new(42);
const WIDTH: u32 = 16;
const HEIGHT: u32 = 16;
const SOURCE_REVISION: u64 = 9;

fn source_identity(fingerprint: &str) -> SourceIdentity {
    SourceIdentity::new(MediaId::from_raw(71), fingerprint).unwrap()
}

fn cache_identity(identity: &SourceIdentity) -> MediaCacheIdentity {
    MediaCacheIdentity::new(identity.media_id(), identity.fingerprint()).unwrap()
}

fn plane(value: u8, stride: usize, rows: u32) -> VideoPlane {
    VideoPlane::new(Arc::from(vec![value; stride * rows as usize]), stride, rows).unwrap()
}

fn frame(timestamp: i64, luma: u8) -> VideoFrame {
    let timebase = Timebase::integer(24).unwrap();
    let format = VideoFormat::new(
        WIDTH,
        HEIGHT,
        PixelFormat::Yuv420p8,
        ColorSpace::BT709,
        AlphaMode::Opaque,
    )
    .unwrap();
    let buffer = CpuVideoBuffer::new(
        WIDTH,
        HEIGHT,
        PixelFormat::Yuv420p8,
        vec![
            plane(luma, WIDTH as usize, HEIGHT),
            plane(96, WIDTH.div_ceil(2) as usize, HEIGHT.div_ceil(2)),
            plane(160, WIDTH.div_ceil(2) as usize, HEIGHT.div_ceil(2)),
        ],
    )
    .unwrap();
    VideoFrame::new(
        format,
        RationalTime::new(timestamp, timebase),
        Duration::new(1, timebase).unwrap(),
        Arc::new(buffer),
    )
    .unwrap()
}

fn proxy_config() -> EncoderConfig {
    EncoderConfig::video(
        PROXY_STREAM,
        CodecId::new("av1").unwrap(),
        Timebase::integer(24).unwrap(),
        frame(0, 32).format(),
    )
}

fn generate_proxy(
    identity: &SourceIdentity,
    source_revision: u64,
    purpose: DerivedMediaPurpose,
    quality: DerivedMediaQuality,
    first_luma: u8,
) -> Arc<DerivedMediaArtifact<EncodedDerivedMedia>> {
    let config = proxy_config();
    let request = DerivedMediaRequest::new(
        cache_identity(identity),
        source_revision,
        purpose,
        quality,
        derived_media_render_settings(&config, purpose, quality).unwrap(),
    );
    let registry = media_backend_registry().unwrap();
    let operation = OperationContext::new(MediaPriority::Background);
    let mut catalog = DerivedMediaCatalog::new();
    generate_derived_media(
        &mut catalog,
        request,
        &registry,
        config,
        vec![
            EncodeInput::Video(frame(0, first_luma)),
            EncodeInput::Video(frame(1, first_luma.saturating_add(16))),
        ],
        &operation,
    )
    .unwrap()
}

fn original_packet() -> Packet {
    Packet::new(
        ORIGINAL_STREAM,
        Arc::from(&b"authoritative-original"[..]),
        PacketTiming::new(Timebase::integer(24).unwrap(), Some(0), Some(0), Some(1)).unwrap(),
    )
    .with_keyframe(true)
}

struct OriginalSource {
    info: SourceInfo,
    packet: Packet,
    consumed: bool,
}

impl OriginalSource {
    fn new(identity: SourceIdentity) -> Self {
        let stream = StreamInfo::new(
            ORIGINAL_STREAM,
            StreamKind::Video,
            CodecId::new("av1").unwrap(),
            Timebase::integer(24).unwrap(),
        );
        Self {
            info: SourceInfo::new(identity, vec![stream]).unwrap(),
            packet: original_packet(),
            consumed: false,
        }
    }
}

impl MediaSource for OriginalSource {
    fn info(&self) -> &SourceInfo {
        &self.info
    }

    fn read_packet(&mut self, operation: &OperationContext) -> Result<ReadOutcome<Packet>> {
        operation.check("read_test_original")?;
        if self.consumed {
            Ok(ReadOutcome::EndOfStream)
        } else {
            self.consumed = true;
            Ok(ReadOutcome::Complete(self.packet.clone()))
        }
    }

    fn seek(&mut self, request: SeekRequest, operation: &OperationContext) -> Result<RationalTime> {
        operation.check("seek_test_original")?;
        self.consumed = false;
        Ok(request.target())
    }
}

#[test]
fn fresh_proxy_selection_is_exact_lower_quality_and_replaceable() {
    let identity = source_identity("sha256:camera-original");
    let quarter = generate_proxy(
        &identity,
        SOURCE_REVISION,
        DerivedMediaPurpose::Proxy,
        DerivedMediaQuality::Quarter,
        32,
    );
    let half = generate_proxy(
        &identity,
        SOURCE_REVISION,
        DerivedMediaPurpose::Proxy,
        DerivedMediaQuality::Half,
        64,
    );
    let operation = OperationContext::new(MediaPriority::Playback);
    let original_opens = AtomicUsize::new(0);
    let artifacts = [Arc::clone(&quarter), Arc::clone(&half)];

    let mut exact = resolve_proxy_source(
        ProxySubstitutionRequest::new(
            identity.clone(),
            SOURCE_REVISION,
            DerivedQuality::Half,
            DerivedFallbackPolicy::ExactOrSource,
        ),
        &artifacts,
        |_| {
            original_opens.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(OriginalSource::new(identity.clone())) as Box<dyn MediaSource>)
        },
        &operation,
    )
    .unwrap();
    assert_eq!(exact.selection().reason(), DerivedSelectionReason::Exact);
    assert_eq!(exact.selection().cache_id(), Some(half.cache_id()));
    assert_eq!(exact.info().identity(), &identity);
    assert_eq!(original_opens.load(Ordering::SeqCst), 0);
    let ReadOutcome::Complete(packet) = exact.read_packet(&operation).unwrap() else {
        panic!("selected proxy did not return its first packet");
    };
    assert_eq!(packet, half.payload().packets()[0]);
    let sought = exact
        .seek(
            SeekRequest::new(
                RationalTime::new(1, Timebase::integer(24).unwrap()),
                SeekMode::Exact,
            ),
            &operation,
        )
        .unwrap();
    assert_eq!(sought, RationalTime::new(1, Timebase::integer(24).unwrap()));
    let ReadOutcome::Complete(preroll) = exact.read_packet(&operation).unwrap() else {
        panic!("exact proxy seek did not return bounded keyframe preroll");
    };
    assert_eq!(preroll, half.payload().packets()[0]);

    let lower = resolve_proxy_source(
        ProxySubstitutionRequest::new(
            identity.clone(),
            SOURCE_REVISION,
            DerivedQuality::Full,
            DerivedFallbackPolicy::LowerQualityOrSource,
        ),
        &artifacts,
        |_| {
            original_opens.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(OriginalSource::new(identity.clone())) as Box<dyn MediaSource>)
        },
        &operation,
    )
    .unwrap();
    assert_eq!(
        lower.selection().reason(),
        DerivedSelectionReason::LowerQuality
    );
    assert_eq!(lower.selection().quality(), Some(DerivedQuality::Half));
    assert_eq!(lower.selection().cache_id(), Some(half.cache_id()));

    let replacement = generate_proxy(
        &identity,
        SOURCE_REVISION,
        DerivedMediaPurpose::Proxy,
        DerivedMediaQuality::Half,
        192,
    );
    assert_ne!(half.cache_id(), replacement.cache_id());
    let mut replaced = resolve_proxy_source(
        ProxySubstitutionRequest::new(
            identity.clone(),
            SOURCE_REVISION,
            DerivedQuality::Half,
            DerivedFallbackPolicy::ExactOrSource,
        ),
        std::slice::from_ref(&replacement),
        |_| {
            original_opens.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(OriginalSource::new(identity.clone())) as Box<dyn MediaSource>)
        },
        &operation,
    )
    .unwrap();
    assert_eq!(
        replaced.selection().cache_id(),
        Some(replacement.cache_id())
    );
    let ReadOutcome::Complete(packet) = replaced.read_packet(&operation).unwrap() else {
        panic!("replacement proxy did not return its first packet");
    };
    assert_eq!(packet, replacement.payload().packets()[0]);
    assert_ne!(packet, half.payload().packets()[0]);

    let expected_tie = std::cmp::min(half.cache_id(), replacement.cache_id());
    for tied in [
        [Arc::clone(&half), Arc::clone(&replacement)],
        [Arc::clone(&replacement), Arc::clone(&half)],
    ] {
        let resolved = resolve_proxy_source(
            ProxySubstitutionRequest::new(
                identity.clone(),
                SOURCE_REVISION,
                DerivedQuality::Half,
                DerivedFallbackPolicy::ExactOrSource,
            ),
            &tied,
            |_| {
                original_opens.fetch_add(1, Ordering::SeqCst);
                Ok(Box::new(OriginalSource::new(identity.clone())) as Box<dyn MediaSource>)
            },
            &operation,
        )
        .unwrap();
        assert_eq!(resolved.selection().cache_id(), Some(expected_tie));
    }
    assert_eq!(original_opens.load(Ordering::SeqCst), 0);
}

#[test]
fn stale_mismatched_optimized_and_higher_quality_media_fall_back_to_original() {
    let identity = source_identity("sha256:camera-original");
    let stale = generate_proxy(
        &identity,
        SOURCE_REVISION - 1,
        DerivedMediaPurpose::Proxy,
        DerivedMediaQuality::Quarter,
        32,
    );
    let wrong_identity = source_identity("sha256:different-content");
    let mismatched = generate_proxy(
        &wrong_identity,
        SOURCE_REVISION,
        DerivedMediaPurpose::Proxy,
        DerivedMediaQuality::Quarter,
        64,
    );
    let optimized = generate_proxy(
        &identity,
        SOURCE_REVISION,
        DerivedMediaPurpose::Optimized,
        DerivedMediaQuality::Quarter,
        96,
    );
    let higher = generate_proxy(
        &identity,
        SOURCE_REVISION,
        DerivedMediaPurpose::Proxy,
        DerivedMediaQuality::Full,
        128,
    );
    let operation = OperationContext::new(MediaPriority::Playback);
    let original_opens = AtomicUsize::new(0);
    let artifacts = [stale, mismatched, optimized, higher];

    let mut resolved = resolve_proxy_source(
        ProxySubstitutionRequest::new(
            identity.clone(),
            SOURCE_REVISION,
            DerivedQuality::Quarter,
            DerivedFallbackPolicy::LowerQualityOrSource,
        ),
        &artifacts,
        |_| {
            original_opens.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(OriginalSource::new(identity.clone())) as Box<dyn MediaSource>)
        },
        &operation,
    )
    .unwrap();

    assert_eq!(
        resolved.selection().reason(),
        DerivedSelectionReason::RequestedQualityUnavailable
    );
    assert!(!resolved.selection().is_derived());
    assert_eq!(resolved.info().identity(), &identity);
    assert_eq!(original_opens.load(Ordering::SeqCst), 1);
    let ReadOutcome::Complete(packet) = resolved.read_packet(&operation).unwrap() else {
        panic!("original fallback did not return its packet");
    };
    assert_eq!(packet, original_packet());

    let empty_opens = AtomicUsize::new(0);
    let empty = resolve_proxy_source(
        ProxySubstitutionRequest::new(
            identity.clone(),
            SOURCE_REVISION,
            DerivedQuality::Quarter,
            DerivedFallbackPolicy::ExactOrSource,
        ),
        &[],
        |_| {
            empty_opens.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(OriginalSource::new(identity.clone())) as Box<dyn MediaSource>)
        },
        &operation,
    )
    .unwrap();
    assert_eq!(
        empty.selection().reason(),
        DerivedSelectionReason::NoFreshCandidate
    );
    assert_eq!(empty_opens.load(Ordering::SeqCst), 1);

    let error = resolve_proxy_source(
        ProxySubstitutionRequest::new(
            identity.clone(),
            SOURCE_REVISION,
            DerivedQuality::Quarter,
            DerivedFallbackPolicy::ExactOrSource,
        ),
        &[],
        |_| {
            Ok(Box::new(OriginalSource::new(source_identity(
                "sha256:wrong-original",
            ))) as Box<dyn MediaSource>)
        },
        &operation,
    )
    .err()
    .expect("mismatched original source unexpectedly resolved");
    assert_eq!(error.category(), ErrorCategory::Conflict);
}

#[test]
fn source_only_policy_keeps_final_delivery_on_authoritative_original() {
    let identity = source_identity("sha256:camera-original");
    let proxy = generate_proxy(
        &identity,
        SOURCE_REVISION,
        DerivedMediaPurpose::Proxy,
        DerivedMediaQuality::Full,
        64,
    );
    let operation = OperationContext::new(MediaPriority::Export);
    let original_opens = AtomicUsize::new(0);

    let mut resolved = resolve_proxy_source(
        ProxySubstitutionRequest::new(
            identity.clone(),
            SOURCE_REVISION,
            DerivedQuality::Full,
            DerivedFallbackPolicy::SourceOnly,
        ),
        &[proxy],
        |_| {
            original_opens.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(OriginalSource::new(identity.clone())) as Box<dyn MediaSource>)
        },
        &operation,
    )
    .unwrap();

    assert_eq!(
        resolved.selection().reason(),
        DerivedSelectionReason::SourceOnlyPolicy
    );
    assert!(!resolved.selection().is_derived());
    assert_eq!(resolved.info().identity(), &identity);
    assert_eq!(original_opens.load(Ordering::SeqCst), 1);
    let ReadOutcome::Complete(packet) = resolved.read_packet(&operation).unwrap() else {
        panic!("source-only delivery did not return the original packet");
    };
    assert_eq!(packet, original_packet());
}
