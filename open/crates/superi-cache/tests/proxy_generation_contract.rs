use std::sync::Arc;

use superi_cache::key::{MediaCacheIdentity, RenderSettingsFingerprint};
use superi_cache::proxy::{
    DerivedMediaCatalog, DerivedMediaLookup, DerivedMediaPurpose, DerivedMediaQuality,
    DerivedMediaRequest, GeneratedMedia,
};
use superi_core::error::{Error, ErrorCategory, Recoverability, Result};
use superi_core::ids::MediaId;

fn media(raw: u128, fingerprint: &str) -> MediaCacheIdentity {
    MediaCacheIdentity::new(MediaId::from_raw(raw), fingerprint).unwrap()
}

fn request(
    media: MediaCacheIdentity,
    revision: u64,
    purpose: DerivedMediaPurpose,
    quality: DerivedMediaQuality,
    settings: &[u8],
) -> DerivedMediaRequest {
    DerivedMediaRequest::new(
        media,
        revision,
        purpose,
        quality,
        RenderSettingsFingerprint::from_canonical_bytes(settings),
    )
}

fn generated(payload: &str, fingerprint: u8) -> Result<GeneratedMedia<String>> {
    GeneratedMedia::new(
        payload.to_owned(),
        [fingerprint; 32],
        u64::try_from(payload.len()).unwrap(),
    )
}

#[test]
fn derived_key_covers_source_freshness_purpose_quality_and_settings() {
    let source = media(7, "sha256:source-a");
    let baseline = request(
        source,
        11,
        DerivedMediaPurpose::Proxy,
        DerivedMediaQuality::Quarter,
        b"av1:yuv420p8:960x540",
    );

    let variants = [
        request(
            media(8, "sha256:source-a"),
            11,
            DerivedMediaPurpose::Proxy,
            DerivedMediaQuality::Quarter,
            b"av1:yuv420p8:960x540",
        ),
        request(
            media(7, "sha256:source-b"),
            11,
            DerivedMediaPurpose::Proxy,
            DerivedMediaQuality::Quarter,
            b"av1:yuv420p8:960x540",
        ),
        request(
            source,
            12,
            DerivedMediaPurpose::Proxy,
            DerivedMediaQuality::Quarter,
            b"av1:yuv420p8:960x540",
        ),
        request(
            source,
            11,
            DerivedMediaPurpose::Optimized,
            DerivedMediaQuality::Quarter,
            b"av1:yuv420p8:960x540",
        ),
        request(
            source,
            11,
            DerivedMediaPurpose::Proxy,
            DerivedMediaQuality::Half,
            b"av1:yuv420p8:960x540",
        ),
        request(
            source,
            11,
            DerivedMediaPurpose::Proxy,
            DerivedMediaQuality::Quarter,
            b"av1:yuv420p10:960x540",
        ),
    ];

    for variant in variants {
        assert_ne!(baseline.key(), variant.key());
    }
    assert_eq!(
        baseline.key(),
        request(
            source,
            11,
            DerivedMediaPurpose::Proxy,
            DerivedMediaQuality::Quarter,
            b"av1:yuv420p8:960x540",
        )
        .key()
    );
    assert_eq!(DerivedMediaQuality::Eighth.code(), "eighth");
    assert_eq!(DerivedMediaQuality::Quarter.code(), "quarter");
    assert_eq!(DerivedMediaQuality::Half.code(), "half");
    assert_eq!(DerivedMediaQuality::Full.code(), "full");
}

#[test]
fn exact_generation_is_reused_and_explicit_regeneration_is_replaceable() {
    let source = media(19, "sha256:camera-original");
    let request = request(
        source,
        3,
        DerivedMediaPurpose::Proxy,
        DerivedMediaQuality::Half,
        b"av1:yuv420p8:1920x1080",
    );
    let mut catalog = DerivedMediaCatalog::new();
    let mut producer_calls = 0;

    let first = catalog
        .get_or_generate(request, || {
            producer_calls += 1;
            generated("first complete payload", 1)
        })
        .unwrap();
    let reused = catalog
        .get_or_generate(request, || {
            producer_calls += 1;
            generated("must not run", 2)
        })
        .unwrap();

    assert_eq!(producer_calls, 1);
    assert!(Arc::ptr_eq(&first, &reused));
    assert_eq!(first.payload(), "first complete payload");
    assert!(first.is_fresh(source, 3));
    assert!(!first.is_fresh(source, 4));
    assert!(!first.is_fresh(media(19, "sha256:changed"), 3));

    let replacement = catalog
        .regenerate(request, || generated("replacement payload", 9))
        .unwrap();
    assert_ne!(first.cache_id(), replacement.cache_id());
    assert_eq!(replacement.payload(), "replacement payload");
    assert_eq!(catalog.len(), 1);
    match catalog.lookup(request) {
        DerivedMediaLookup::Generated(found) => assert!(Arc::ptr_eq(&found, &replacement)),
        DerivedMediaLookup::OriginalSource(_) => panic!("current generated media was not found"),
    }
}

#[test]
fn failure_never_publishes_partial_output_or_discards_a_previous_artifact() {
    let source = media(23, "sha256:source");
    let current = request(
        source,
        5,
        DerivedMediaPurpose::Optimized,
        DerivedMediaQuality::Full,
        b"av1:yuv420p10:3840x2160",
    );
    let stale = request(
        source,
        4,
        DerivedMediaPurpose::Optimized,
        DerivedMediaQuality::Full,
        b"av1:yuv420p10:3840x2160",
    );
    let mut catalog = DerivedMediaCatalog::new();
    let original = catalog
        .regenerate(current, || generated("complete optimized media", 3))
        .unwrap();

    let failure = catalog.regenerate(current, || {
        Err(Error::new(
            ErrorCategory::Cancelled,
            Recoverability::Degraded,
            "generation cancelled before complete publication",
        ))
    });
    assert_eq!(failure.unwrap_err().category(), ErrorCategory::Cancelled);
    assert_eq!(catalog.len(), 1);
    match catalog.lookup(current) {
        DerivedMediaLookup::Generated(found) => assert!(Arc::ptr_eq(&found, &original)),
        DerivedMediaLookup::OriginalSource(_) => panic!("previous complete media was discarded"),
    }

    match catalog.lookup(stale) {
        DerivedMediaLookup::OriginalSource(identity) => assert_eq!(identity, source),
        DerivedMediaLookup::Generated(_) => panic!("stale revision reused generated media"),
    }

    let empty = GeneratedMedia::new(String::new(), [0; 32], 0).unwrap_err();
    assert_eq!(empty.category(), ErrorCategory::InvalidInput);
}

#[test]
fn catalog_inspection_and_clear_preserve_exact_identity_and_source_fallback() {
    let source = media(31, "sha256:management-source");
    let quarter = request(
        source,
        7,
        DerivedMediaPurpose::Proxy,
        DerivedMediaQuality::Quarter,
        b"av1:yuv420p8:960x540",
    );
    let half = request(
        source,
        7,
        DerivedMediaPurpose::Proxy,
        DerivedMediaQuality::Half,
        b"av1:yuv420p8:1920x1080",
    );
    let mut catalog = DerivedMediaCatalog::new();
    catalog
        .regenerate(half, || generated("half payload", 5))
        .unwrap();
    catalog
        .regenerate(quarter, || generated("quarter payload", 4))
        .unwrap();

    let inspection = catalog.inspect();
    assert_eq!(inspection.keys(), &[quarter.key(), half.key()]);
    assert_eq!(inspection.total_bytes(), 27);
    assert_eq!(inspection.entries()[0].request().media(), source);
    assert_eq!(inspection.entries()[0].request().source_revision(), 7);
    assert_eq!(
        inspection.entries()[0].request().quality(),
        DerivedMediaQuality::Quarter
    );
    assert_eq!(inspection.entries()[0].byte_len(), 15);

    let cleared = catalog.clear();
    assert_eq!(cleared.removed_keys(), inspection.keys());
    assert_eq!(cleared.removed_entries(), inspection.entries());
    assert_eq!(cleared.removed_bytes(), inspection.total_bytes());
    assert!(catalog.is_empty());
    match catalog.lookup(quarter) {
        DerivedMediaLookup::OriginalSource(identity) => assert_eq!(identity, source),
        DerivedMediaLookup::Generated(_) => panic!("cleared derived media remained reusable"),
    }
}
