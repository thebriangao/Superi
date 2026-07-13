use superi_concurrency::jobs::{
    DerivedFallbackPolicy, DerivedMediaCandidate, DerivedMediaRequest, DerivedQuality,
    DerivedSelectionReason, JobKind, JobPriority, PriorityScheduler, ScheduledJob,
};
use superi_core::error::ErrorCategory;
use superi_core::ids::{CacheId, JobId, MediaId};

fn job(raw: u128, priority: JobPriority, payload: &'static str) -> ScheduledJob<&'static str> {
    ScheduledJob::new(JobId::from_raw(raw), JobKind::Frame, priority, payload)
}

#[test]
fn work_kinds_and_priorities_have_stable_public_codes() {
    assert_eq!(
        JobKind::ALL,
        &[
            JobKind::Frame,
            JobKind::Tile,
            JobKind::Proxy,
            JobKind::Analysis,
            JobKind::Cache,
            JobKind::Export,
        ]
    );
    assert_eq!(
        JobKind::ALL
            .iter()
            .map(|kind| kind.code())
            .collect::<Vec<_>>(),
        ["frame", "tile", "proxy", "analysis", "cache", "export"]
    );
    for kind in JobKind::ALL {
        assert_eq!(JobKind::from_code(kind.code()), Some(*kind));
    }
    assert_eq!(JobKind::from_code("unknown"), None);

    assert_eq!(
        JobPriority::ALL,
        &[
            JobPriority::Background,
            JobPriority::Export,
            JobPriority::Playback,
            JobPriority::Interactive,
        ]
    );
    assert_eq!(
        JobPriority::ALL
            .iter()
            .map(|priority| (
                priority.code(),
                priority.rank(),
                priority.service_weight(),
                priority.maximum_waiting_dispatches(),
            ))
            .collect::<Vec<_>>(),
        [
            ("background", 0, 1, 14),
            ("export", 1, 2, 7),
            ("playback", 2, 4, 3),
            ("interactive", 3, 8, 1),
        ]
    );
    for priority in JobPriority::ALL {
        assert_eq!(JobPriority::from_code(priority.code()), Some(*priority));
    }
    assert_eq!(JobPriority::from_code("unknown"), None);
}

#[test]
fn scheduler_preserves_fifo_identity_and_payload_within_a_priority() {
    let mut scheduler = PriorityScheduler::new();
    scheduler
        .enqueue(job(1, JobPriority::Playback, "first"))
        .unwrap();
    scheduler
        .enqueue(job(2, JobPriority::Playback, "second"))
        .unwrap();
    scheduler
        .enqueue(job(3, JobPriority::Playback, "third"))
        .unwrap();

    assert_eq!(scheduler.len(), 3);
    assert_eq!(scheduler.len_for(JobPriority::Playback), 3);
    assert_eq!(scheduler.len_for(JobPriority::Interactive), 0);

    for (expected_id, expected_payload) in [(1, "first"), (2, "second"), (3, "third")] {
        let next = scheduler.next_job().unwrap();
        assert_eq!(next.id(), JobId::from_raw(expected_id));
        assert_eq!(next.kind(), JobKind::Frame);
        assert_eq!(next.priority(), JobPriority::Playback);
        assert_eq!(*next.payload(), expected_payload);
    }
    assert!(scheduler.next_job().is_none());
    assert!(scheduler.is_empty());
}

#[test]
fn scheduler_applies_deterministic_weighted_service_without_starvation() {
    let mut scheduler = PriorityScheduler::new();
    let priorities = [
        JobPriority::Background,
        JobPriority::Export,
        JobPriority::Playback,
        JobPriority::Interactive,
    ];
    let mut raw = 1_u128;
    for priority in priorities {
        for _ in 0..30 {
            scheduler
                .enqueue(job(raw, priority, priority.code()))
                .unwrap();
            raw += 1;
        }
    }

    let first_cycle = (0..15)
        .map(|_| scheduler.next_job().unwrap().priority())
        .collect::<Vec<_>>();
    assert_eq!(
        first_cycle,
        [
            JobPriority::Interactive,
            JobPriority::Playback,
            JobPriority::Interactive,
            JobPriority::Export,
            JobPriority::Interactive,
            JobPriority::Playback,
            JobPriority::Interactive,
            JobPriority::Background,
            JobPriority::Interactive,
            JobPriority::Playback,
            JobPriority::Interactive,
            JobPriority::Export,
            JobPriority::Interactive,
            JobPriority::Playback,
            JobPriority::Interactive,
        ]
    );

    let second_cycle = (0..15)
        .map(|_| scheduler.next_job().unwrap().priority())
        .collect::<Vec<_>>();
    assert_eq!(second_cycle, first_cycle);
}

#[test]
fn scheduler_skips_empty_classes_without_losing_priority_or_fifo_order() {
    let mut scheduler = PriorityScheduler::new();
    for raw in 1..=9 {
        scheduler
            .enqueue(job(raw, JobPriority::Interactive, "interactive"))
            .unwrap();
    }
    scheduler
        .enqueue(job(10, JobPriority::Background, "background"))
        .unwrap();

    let order = (0..10)
        .map(|_| scheduler.next_job().unwrap().id().raw())
        .collect::<Vec<_>>();
    assert_eq!(order, [1, 2, 3, 4, 10, 5, 6, 7, 8, 9]);
}

#[test]
fn scheduler_rejects_duplicate_queued_identity_but_allows_a_new_lifetime_after_dispatch() {
    let mut scheduler = PriorityScheduler::new();
    scheduler
        .enqueue(job(7, JobPriority::Background, "old"))
        .unwrap();

    let error = scheduler
        .enqueue(job(7, JobPriority::Interactive, "duplicate"))
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(
        error.contexts()[0].field("job_id"),
        Some("job:00000000000000000000000000000007")
    );
    assert_eq!(scheduler.len(), 1);

    assert_eq!(scheduler.next_job().unwrap().into_payload(), "old");
    scheduler
        .enqueue(job(7, JobPriority::Interactive, "new"))
        .unwrap();
    assert_eq!(scheduler.next_job().unwrap().into_payload(), "new");
}

#[test]
fn scheduled_job_carries_every_work_kind_and_explicit_derived_media_intent() {
    let request = DerivedMediaRequest::new(
        MediaId::from_raw(20),
        9,
        DerivedQuality::Half,
        DerivedFallbackPolicy::LowerQualityOrSource,
    );

    for (index, kind) in JobKind::ALL.iter().copied().enumerate() {
        let scheduled = ScheduledJob::new(
            JobId::from_raw(index as u128 + 1),
            kind,
            JobPriority::Background,
            (),
        )
        .with_derived_media(request);
        assert_eq!(scheduled.kind(), kind);
        assert_eq!(scheduled.derived_media(), Some(&request));
    }
}

#[test]
fn derived_media_selects_exact_fresh_identity_with_deterministic_ties() {
    assert_eq!(
        DerivedQuality::ALL,
        &[
            DerivedQuality::Eighth,
            DerivedQuality::Quarter,
            DerivedQuality::Half,
            DerivedQuality::Full,
        ]
    );
    assert_eq!(
        DerivedQuality::ALL
            .iter()
            .map(|quality| (quality.code(), quality.rank()))
            .collect::<Vec<_>>(),
        [("eighth", 0), ("quarter", 1), ("half", 2), ("full", 3)]
    );
    for quality in DerivedQuality::ALL {
        assert_eq!(DerivedQuality::from_code(quality.code()), Some(*quality));
    }
    assert_eq!(DerivedQuality::from_code("unknown"), None);
    for policy in DerivedFallbackPolicy::ALL {
        assert_eq!(
            DerivedFallbackPolicy::from_code(policy.code()),
            Some(*policy)
        );
    }
    assert_eq!(DerivedFallbackPolicy::from_code("unknown"), None);
    assert_eq!(DerivedSelectionReason::Exact.code(), "exact");
    assert_eq!(
        DerivedSelectionReason::RequestedQualityUnavailable.code(),
        "requested_quality_unavailable"
    );

    let source = MediaId::from_raw(40);
    let request = DerivedMediaRequest::new(
        source,
        12,
        DerivedQuality::Half,
        DerivedFallbackPolicy::ExactOrSource,
    );
    let candidates = [
        DerivedMediaCandidate::new(CacheId::from_raw(9), source, 12, DerivedQuality::Half),
        DerivedMediaCandidate::new(CacheId::from_raw(3), source, 12, DerivedQuality::Half),
        DerivedMediaCandidate::new(CacheId::from_raw(1), source, 11, DerivedQuality::Half),
        DerivedMediaCandidate::new(
            CacheId::from_raw(2),
            MediaId::from_raw(41),
            12,
            DerivedQuality::Half,
        ),
    ];

    let selected = request.select(&candidates);
    assert_eq!(selected.source(), source);
    assert_eq!(selected.source_revision(), 12);
    assert_eq!(selected.requested_quality(), DerivedQuality::Half);
    assert_eq!(selected.cache_id(), Some(CacheId::from_raw(3)));
    assert_eq!(selected.quality(), Some(DerivedQuality::Half));
    assert_eq!(selected.reason(), DerivedSelectionReason::Exact);
    assert!(selected.is_derived());
    assert!(selected.is_exact());
}

#[test]
fn derived_media_uses_nearest_lower_quality_then_authoritative_source() {
    let source = MediaId::from_raw(50);
    let request = DerivedMediaRequest::new(
        source,
        4,
        DerivedQuality::Full,
        DerivedFallbackPolicy::LowerQualityOrSource,
    );
    let candidates = [
        DerivedMediaCandidate::new(CacheId::from_raw(1), source, 4, DerivedQuality::Eighth),
        DerivedMediaCandidate::new(CacheId::from_raw(8), source, 4, DerivedQuality::Half),
        DerivedMediaCandidate::new(CacheId::from_raw(2), source, 4, DerivedQuality::Half),
        DerivedMediaCandidate::new(CacheId::from_raw(3), source, 3, DerivedQuality::Full),
    ];

    let selected = request.select(&candidates);
    assert_eq!(selected.cache_id(), Some(CacheId::from_raw(2)));
    assert_eq!(selected.quality(), Some(DerivedQuality::Half));
    assert_eq!(selected.reason(), DerivedSelectionReason::LowerQuality);
    assert!(selected.is_derived());
    assert!(!selected.is_exact());

    let exact_only = DerivedMediaRequest::new(
        source,
        4,
        DerivedQuality::Full,
        DerivedFallbackPolicy::ExactOrSource,
    )
    .select(&candidates);
    assert_eq!(exact_only.cache_id(), None);
    assert_eq!(exact_only.quality(), None);
    assert_eq!(
        exact_only.reason(),
        DerivedSelectionReason::RequestedQualityUnavailable
    );
    assert!(!exact_only.is_derived());
}

#[test]
fn derived_media_reports_stale_misses_and_source_only_policy_transparently() {
    let source = MediaId::from_raw(60);
    let stale = [DerivedMediaCandidate::new(
        CacheId::from_raw(1),
        source,
        6,
        DerivedQuality::Quarter,
    )];

    let missing = DerivedMediaRequest::new(
        source,
        7,
        DerivedQuality::Quarter,
        DerivedFallbackPolicy::LowerQualityOrSource,
    )
    .select(&stale);
    assert_eq!(missing.cache_id(), None);
    assert_eq!(missing.reason(), DerivedSelectionReason::NoFreshCandidate);
    assert_eq!(missing.source(), source);
    assert_eq!(missing.source_revision(), 7);

    let source_only = DerivedMediaRequest::new(
        source,
        7,
        DerivedQuality::Quarter,
        DerivedFallbackPolicy::SourceOnly,
    )
    .select(&[DerivedMediaCandidate::new(
        CacheId::from_raw(2),
        source,
        7,
        DerivedQuality::Quarter,
    )]);
    assert_eq!(source_only.cache_id(), None);
    assert_eq!(
        source_only.reason(),
        DerivedSelectionReason::SourceOnlyPolicy
    );
    assert!(!source_only.is_derived());
}
