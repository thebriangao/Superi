use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{mpsc, Arc, Barrier, Mutex};
use std::thread;

use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_core::time::{Duration, RationalTime, Timebase};
use superi_engine::resource_arbitration::{
    EngineResourceArbiter, ResourceAdmission, ResourceArbitrationConfig, ResourceClass,
    ResourceClassBudget, ResourceConsumer, ResourceDegradation, ResourceFallback, ResourceGrant,
    ResourcePressureKind, ResourceReclaimRequest, ResourceReclaimStatus, ResourceReclaimer,
    ResourceRequest, ResourceReservation,
};
use superi_media_io::decode::{CpuVideoBuffer, VideoFormat, VideoFrame, VideoPlane};
use superi_media_io::demux::MetadataValue;

fn arbitration_config(total_bytes: u64) -> Result<ResourceArbitrationConfig> {
    ResourceArbitrationConfig::new(
        total_bytes,
        [
            ResourceClassBudget::new(ResourceClass::DecodeBuffer, 8, total_bytes)?,
            ResourceClassBudget::new(ResourceClass::GpuMemory, 16, total_bytes)?,
            ResourceClassBudget::new(ResourceClass::Cache, 0, total_bytes)?,
            ResourceClassBudget::new(ResourceClass::Audio, 8, total_bytes)?,
            ResourceClassBudget::new(ResourceClass::Ai, 0, total_bytes)?,
            ResourceClassBudget::new(ResourceClass::Export, 0, total_bytes)?,
        ],
    )
}

fn decoded_frame() -> Result<VideoFrame> {
    let format = VideoFormat::new(
        2,
        2,
        PixelFormat::Rgba16Float,
        ColorSpace::SRGB,
        AlphaMode::Straight,
    )?;
    let timebase = Timebase::integer(24)?;
    let plane = VideoPlane::new(Arc::from([0_u8; 32]), 16, 2)?;
    let buffer = Arc::new(CpuVideoBuffer::new(
        2,
        2,
        PixelFormat::Rgba16Float,
        vec![plane],
    )?);
    VideoFrame::new(
        format,
        RationalTime::new(7, timebase),
        Duration::new(1, timebase)?,
        buffer,
    )?
    .with_metadata("source.frame", MetadataValue::Unsigned(7))
}

fn granted(admission: ResourceAdmission) -> ResourceGrant {
    match admission {
        ResourceAdmission::Granted(grant) => grant,
        ResourceAdmission::Degraded(degradation) => {
            panic!("expected admission, got {degradation:?}")
        }
    }
}

fn degraded(admission: ResourceAdmission) -> ResourceDegradation {
    match admission {
        ResourceAdmission::Degraded(degradation) => degradation,
        ResourceAdmission::Granted(grant) => panic!("expected degradation, got {grant:?}"),
    }
}

#[derive(Default)]
struct LeaseReclaimer {
    leases: Mutex<Vec<ResourceReservation>>,
    calls: AtomicUsize,
}

impl LeaseReclaimer {
    fn hold(&self, reservation: ResourceReservation) {
        self.leases
            .lock()
            .expect("test lease holder is not poisoned")
            .push(reservation);
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl ResourceReclaimer for LeaseReclaimer {
    fn reclaim(&self, request: ResourceReclaimRequest) -> Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let mut released = 0_u64;
        let mut leases = self
            .leases
            .lock()
            .expect("test lease holder is not poisoned");
        while released < request.bytes_to_release() {
            let Some(reservation) = leases.pop() else {
                break;
            };
            released = released.saturating_add(reservation.bytes());
            drop(reservation);
        }
        Ok(())
    }
}

#[derive(Default)]
struct FailingReclaimer {
    calls: AtomicUsize,
}

impl ResourceReclaimer for FailingReclaimer {
    fn reclaim(&self, _request: ResourceReclaimRequest) -> Result<()> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        Err(Error::new(
            ErrorCategory::Unavailable,
            Recoverability::Degraded,
            "cache eviction owner is temporarily unavailable",
        )
        .with_context(ErrorContext::new("test.resource", "evict_cache")))
    }
}

struct ReentrantReclaimer {
    arbiter: EngineResourceArbiter,
    leases: Mutex<Vec<ResourceReservation>>,
    observed: Mutex<Option<(ErrorCategory, Recoverability)>>,
}

impl ReentrantReclaimer {
    fn new(arbiter: EngineResourceArbiter, reservation: ResourceReservation) -> Self {
        Self {
            arbiter,
            leases: Mutex::new(vec![reservation]),
            observed: Mutex::new(None),
        }
    }
}

impl ResourceReclaimer for ReentrantReclaimer {
    fn reclaim(&self, request: ResourceReclaimRequest) -> Result<()> {
        let reentrant = self.arbiter.reserve(ResourceRequest::new(
            request.class(),
            ResourceConsumer::Recovery,
            1,
        )?);
        let error = reentrant.expect_err("reentrant admission must fail before waiting");
        *self
            .observed
            .lock()
            .expect("test observation lock is not poisoned") =
            Some((error.category(), error.recoverability()));
        self.leases
            .lock()
            .expect("test lease holder is not poisoned")
            .clear();
        Ok(())
    }
}

#[test]
fn exact_admission_binds_a_real_decoded_frame_without_semantic_drift() -> Result<()> {
    let arbiter = EngineResourceArbiter::new(arbitration_config(256)?);
    let frame = decoded_frame()?;
    let expected_format = frame.format();
    let expected_timestamp = frame.timestamp();
    let expected_duration = frame.duration();
    let expected_metadata = frame.metadata().clone();
    let expected_color = frame.color_pipeline().clone();

    let admission = arbiter.reserve(ResourceRequest::new(
        ResourceClass::DecodeBuffer,
        ResourceConsumer::Playback,
        32,
    )?)?;
    let ResourceAdmission::Granted(grant) = admission else {
        panic!("an empty finite arbiter must admit the decoded frame")
    };
    let retained = grant.bind(frame);

    assert_eq!(retained.value().format(), expected_format);
    assert_eq!(retained.value().timestamp(), expected_timestamp);
    assert_eq!(retained.value().duration(), expected_duration);
    assert_eq!(retained.value().metadata(), &expected_metadata);
    assert_eq!(retained.value().color_pipeline(), &expected_color);
    assert_eq!(retained.reservation().class(), ResourceClass::DecodeBuffer);
    assert_eq!(
        retained.reservation().consumer(),
        ResourceConsumer::Playback
    );
    assert_eq!(retained.reservation().bytes(), 32);

    let snapshot = arbiter.snapshot()?;
    assert_eq!(snapshot.total_used_bytes(), 32);
    assert_eq!(snapshot.active_reservations(), 1);
    assert_eq!(snapshot.class(ResourceClass::DecodeBuffer).used_bytes(), 32);
    drop(retained);

    let released = arbiter.snapshot()?;
    assert_eq!(released.total_used_bytes(), 0);
    assert_eq!(released.active_reservations(), 0);
    assert_eq!(released.class(ResourceClass::DecodeBuffer).used_bytes(), 0);
    assert_eq!(released.revision(), 2);
    Ok(())
}

#[test]
fn configuration_rejects_incomplete_duplicate_and_overcommitted_budgets() -> Result<()> {
    assert_eq!(
        ResourceClassBudget::new(ResourceClass::Audio, 2, 1)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    let valid = arbitration_config(64)?;
    assert_eq!(valid.total_bytes(), 64);
    assert_eq!(valid.budget(ResourceClass::GpuMemory).protected_bytes(), 16);

    let duplicate = ResourceArbitrationConfig::new(
        64,
        [
            ResourceClassBudget::new(ResourceClass::DecodeBuffer, 0, 64)?,
            ResourceClassBudget::new(ResourceClass::GpuMemory, 0, 64)?,
            ResourceClassBudget::new(ResourceClass::Cache, 0, 64)?,
            ResourceClassBudget::new(ResourceClass::Audio, 0, 64)?,
            ResourceClassBudget::new(ResourceClass::Ai, 0, 64)?,
            ResourceClassBudget::new(ResourceClass::Ai, 0, 64)?,
        ],
    )
    .unwrap_err();
    assert_eq!(duplicate.category(), ErrorCategory::InvalidInput);

    let incomplete = ResourceArbitrationConfig::new(
        64,
        [
            ResourceClassBudget::new(ResourceClass::DecodeBuffer, 0, 64)?,
            ResourceClassBudget::new(ResourceClass::GpuMemory, 0, 64)?,
            ResourceClassBudget::new(ResourceClass::Cache, 0, 64)?,
            ResourceClassBudget::new(ResourceClass::Audio, 0, 64)?,
            ResourceClassBudget::new(ResourceClass::Ai, 0, 64)?,
        ],
    )
    .unwrap_err();
    assert_eq!(incomplete.category(), ErrorCategory::InvalidInput);

    let overcommitted = ResourceArbitrationConfig::new(
        64,
        [
            ResourceClassBudget::new(ResourceClass::DecodeBuffer, 16, 64)?,
            ResourceClassBudget::new(ResourceClass::GpuMemory, 16, 64)?,
            ResourceClassBudget::new(ResourceClass::Cache, 16, 64)?,
            ResourceClassBudget::new(ResourceClass::Audio, 16, 64)?,
            ResourceClassBudget::new(ResourceClass::Ai, 1, 64)?,
            ResourceClassBudget::new(ResourceClass::Export, 0, 64)?,
        ],
    )
    .unwrap_err();
    assert_eq!(overcommitted.category(), ErrorCategory::InvalidInput);

    let oversized_class = ResourceArbitrationConfig::new(
        64,
        [
            ResourceClassBudget::new(ResourceClass::DecodeBuffer, 0, 65)?,
            ResourceClassBudget::new(ResourceClass::GpuMemory, 0, 64)?,
            ResourceClassBudget::new(ResourceClass::Cache, 0, 64)?,
            ResourceClassBudget::new(ResourceClass::Audio, 0, 64)?,
            ResourceClassBudget::new(ResourceClass::Ai, 0, 64)?,
            ResourceClassBudget::new(ResourceClass::Export, 0, 64)?,
        ],
    )
    .unwrap_err();
    assert_eq!(oversized_class.category(), ErrorCategory::InvalidInput);
    Ok(())
}

#[test]
fn pressure_reclaims_borrowed_cache_before_protected_audio_or_gpu_capacity() -> Result<()> {
    let arbiter = EngineResourceArbiter::new(arbitration_config(100)?);
    let cache = Arc::new(LeaseReclaimer::default());
    let ai = Arc::new(LeaseReclaimer::default());
    arbiter.set_reclaimer(ResourceClass::Cache, Some(cache.clone()))?;
    arbiter.set_reclaimer(ResourceClass::Ai, Some(ai.clone()))?;

    cache.hold(
        granted(arbiter.reserve(ResourceRequest::new(
            ResourceClass::Cache,
            ResourceConsumer::Background,
            60,
        )?)?)
        .into_reservation(),
    );
    let audio = granted(arbiter.reserve(ResourceRequest::new(
        ResourceClass::Audio,
        ResourceConsumer::Playback,
        20,
    )?)?)
    .into_reservation();

    let gpu = granted(arbiter.reserve(ResourceRequest::new(
        ResourceClass::GpuMemory,
        ResourceConsumer::Render,
        30,
    )?)?);
    assert_eq!(cache.calls(), 1);
    assert_eq!(ai.calls(), 0);
    assert_eq!(gpu.evidence().used_before_bytes(), 80);
    assert_eq!(gpu.evidence().reclaimed_bytes(), 60);
    assert_eq!(gpu.evidence().attempts().len(), 1);
    assert_eq!(gpu.evidence().attempts()[0].class(), ResourceClass::Cache);
    assert_eq!(
        gpu.evidence().attempts()[0].status(),
        ResourceReclaimStatus::Released
    );
    let snapshot = arbiter.snapshot()?;
    assert_eq!(snapshot.total_used_bytes(), 50);
    assert_eq!(snapshot.class(ResourceClass::Audio).used_bytes(), 20);
    assert_eq!(snapshot.class(ResourceClass::GpuMemory).used_bytes(), 30);
    drop(gpu);
    drop(audio);
    Ok(())
}

#[test]
fn global_reclaim_skips_a_class_at_its_protected_floor() -> Result<()> {
    let config = ResourceArbitrationConfig::new(
        100,
        [
            ResourceClassBudget::new(ResourceClass::DecodeBuffer, 8, 100)?,
            ResourceClassBudget::new(ResourceClass::GpuMemory, 16, 100)?,
            ResourceClassBudget::new(ResourceClass::Cache, 20, 100)?,
            ResourceClassBudget::new(ResourceClass::Audio, 8, 100)?,
            ResourceClassBudget::new(ResourceClass::Ai, 0, 100)?,
            ResourceClassBudget::new(ResourceClass::Export, 0, 100)?,
        ],
    )?;
    let arbiter = EngineResourceArbiter::new(config);
    let cache = Arc::new(LeaseReclaimer::default());
    let ai = Arc::new(LeaseReclaimer::default());
    arbiter.set_reclaimer(ResourceClass::Cache, Some(cache.clone()))?;
    arbiter.set_reclaimer(ResourceClass::Ai, Some(ai.clone()))?;
    cache.hold(
        granted(arbiter.reserve(ResourceRequest::new(
            ResourceClass::Cache,
            ResourceConsumer::Background,
            20,
        )?)?)
        .into_reservation(),
    );
    ai.hold(
        granted(arbiter.reserve(ResourceRequest::new(
            ResourceClass::Ai,
            ResourceConsumer::Background,
            60,
        )?)?)
        .into_reservation(),
    );

    let gpu = granted(arbiter.reserve(ResourceRequest::new(
        ResourceClass::GpuMemory,
        ResourceConsumer::Render,
        30,
    )?)?);
    assert_eq!(cache.calls(), 0);
    assert_eq!(ai.calls(), 1);
    assert_eq!(gpu.evidence().attempts().len(), 1);
    assert_eq!(gpu.evidence().attempts()[0].class(), ResourceClass::Ai);
    let snapshot = arbiter.snapshot()?;
    assert_eq!(snapshot.class(ResourceClass::Cache).used_bytes(), 20);
    assert_eq!(snapshot.class(ResourceClass::GpuMemory).used_bytes(), 30);
    drop(gpu);
    arbiter.set_reclaimer(ResourceClass::Cache, None)?;
    Ok(())
}

#[test]
fn callback_failure_is_retained_and_cooperation_continues_in_fixed_order() -> Result<()> {
    let arbiter = EngineResourceArbiter::new(arbitration_config(100)?);
    let failing = Arc::new(FailingReclaimer::default());
    let ai = Arc::new(LeaseReclaimer::default());
    arbiter.set_reclaimer(ResourceClass::Cache, Some(failing.clone()))?;
    arbiter.set_reclaimer(ResourceClass::Ai, Some(ai.clone()))?;

    let cache = granted(arbiter.reserve(ResourceRequest::new(
        ResourceClass::Cache,
        ResourceConsumer::Background,
        40,
    )?)?)
    .into_reservation();
    ai.hold(
        granted(arbiter.reserve(ResourceRequest::new(
            ResourceClass::Ai,
            ResourceConsumer::Background,
            40,
        )?)?)
        .into_reservation(),
    );
    let audio = granted(arbiter.reserve(ResourceRequest::new(
        ResourceClass::Audio,
        ResourceConsumer::Playback,
        10,
    )?)?)
    .into_reservation();

    let gpu = granted(arbiter.reserve(ResourceRequest::new(
        ResourceClass::GpuMemory,
        ResourceConsumer::Render,
        30,
    )?)?);
    assert_eq!(failing.calls.load(Ordering::SeqCst), 1);
    assert_eq!(ai.calls(), 1);
    assert_eq!(gpu.evidence().attempts().len(), 2);
    assert_eq!(
        gpu.evidence().attempts()[0].status(),
        ResourceReclaimStatus::Failed
    );
    assert_eq!(
        gpu.evidence().attempts()[0]
            .failure()
            .expect("classified callback failure is retained")
            .category(),
        ErrorCategory::Unavailable
    );
    assert_eq!(
        gpu.evidence().attempts()[1].status(),
        ResourceReclaimStatus::Released
    );
    drop(gpu);
    drop(audio);
    drop(cache);
    Ok(())
}

#[test]
fn every_resource_class_has_a_semantic_consumer_aware_fallback() -> Result<()> {
    let arbiter = EngineResourceArbiter::new(arbitration_config(64)?);
    let blockers = [
        (ResourceClass::DecodeBuffer, 10),
        (ResourceClass::GpuMemory, 10),
        (ResourceClass::Cache, 10),
        (ResourceClass::Audio, 10),
        (ResourceClass::Ai, 10),
        (ResourceClass::Export, 14),
    ]
    .into_iter()
    .map(|(class, bytes)| {
        arbiter
            .reserve(ResourceRequest::new(
                class,
                ResourceConsumer::Background,
                bytes,
            )?)
            .map(granted)
            .map(ResourceGrant::into_reservation)
    })
    .collect::<Result<Vec<_>>>()?;

    for (class, consumer, expected) in [
        (
            ResourceClass::DecodeBuffer,
            ResourceConsumer::Playback,
            ResourceFallback::ReduceDecodeLookahead,
        ),
        (
            ResourceClass::GpuMemory,
            ResourceConsumer::Render,
            ResourceFallback::DeferGpuWork,
        ),
        (
            ResourceClass::Cache,
            ResourceConsumer::Export,
            ResourceFallback::BypassCache,
        ),
        (
            ResourceClass::Audio,
            ResourceConsumer::Playback,
            ResourceFallback::PreservePlaybackClockWithSilence,
        ),
        (
            ResourceClass::Audio,
            ResourceConsumer::Export,
            ResourceFallback::PauseExport,
        ),
        (
            ResourceClass::Ai,
            ResourceConsumer::Background,
            ResourceFallback::DeferAi,
        ),
        (
            ResourceClass::Export,
            ResourceConsumer::Export,
            ResourceFallback::PauseExport,
        ),
    ] {
        let degradation = degraded(arbiter.reserve(ResourceRequest::new(class, consumer, 1)?)?);
        assert_eq!(degradation.request().class(), class);
        assert_eq!(degradation.request().consumer(), consumer);
        assert_eq!(degradation.fallback(), expected);
        assert_eq!(
            degradation.pressure_kind(),
            ResourcePressureKind::GlobalBudget
        );
        assert_eq!(degradation.shortage_bytes(), 1);
    }
    assert_eq!(arbiter.snapshot()?.denied_reservations(), 7);
    drop(blockers);
    Ok(())
}

#[test]
fn a_class_ceiling_denies_then_recovers_without_consuming_global_headroom() -> Result<()> {
    let config = ResourceArbitrationConfig::new(
        100,
        [
            ResourceClassBudget::new(ResourceClass::DecodeBuffer, 10, 100)?,
            ResourceClassBudget::new(ResourceClass::GpuMemory, 10, 100)?,
            ResourceClassBudget::new(ResourceClass::Cache, 0, 30)?,
            ResourceClassBudget::new(ResourceClass::Audio, 10, 100)?,
            ResourceClassBudget::new(ResourceClass::Ai, 0, 100)?,
            ResourceClassBudget::new(ResourceClass::Export, 0, 100)?,
        ],
    )?;
    let arbiter = EngineResourceArbiter::new(config);
    let first = granted(arbiter.reserve(ResourceRequest::new(
        ResourceClass::Cache,
        ResourceConsumer::Background,
        20,
    )?)?)
    .into_reservation();

    let denial = degraded(arbiter.reserve(ResourceRequest::new(
        ResourceClass::Cache,
        ResourceConsumer::Background,
        20,
    )?)?);
    assert_eq!(denial.pressure_kind(), ResourcePressureKind::ClassBudget);
    assert_eq!(denial.total_available_bytes(), 80);
    assert_eq!(denial.class_available_bytes(), 10);
    assert_eq!(denial.shortage_bytes(), 10);
    drop(first);

    let recovered = granted(arbiter.reserve(ResourceRequest::new(
        ResourceClass::Cache,
        ResourceConsumer::Recovery,
        20,
    )?)?);
    assert_eq!(recovered.reservation().bytes(), 20);
    let snapshot = arbiter.snapshot()?;
    assert_eq!(
        snapshot.class(ResourceClass::Cache).denied_reservations(),
        1
    );
    assert_eq!(snapshot.total_used_bytes(), 20);
    drop(recovered);
    Ok(())
}

#[test]
fn an_impossible_single_request_degrades_without_evicting_valid_work() -> Result<()> {
    let arbiter = EngineResourceArbiter::new(arbitration_config(64)?);
    let cache = Arc::new(LeaseReclaimer::default());
    arbiter.set_reclaimer(ResourceClass::Cache, Some(cache.clone()))?;
    cache.hold(
        granted(arbiter.reserve(ResourceRequest::new(
            ResourceClass::Cache,
            ResourceConsumer::Background,
            64,
        )?)?)
        .into_reservation(),
    );

    let degradation = degraded(arbiter.reserve(ResourceRequest::new(
        ResourceClass::GpuMemory,
        ResourceConsumer::Render,
        65,
    )?)?);
    assert_eq!(degradation.fallback(), ResourceFallback::DeferGpuWork);
    assert_eq!(
        degradation.pressure_kind(),
        ResourcePressureKind::GlobalAndClassBudget
    );
    assert!(degradation.evidence().attempts().is_empty());
    assert_eq!(cache.calls(), 0);
    assert_eq!(arbiter.snapshot()?.total_used_bytes(), 64);

    arbiter.set_reclaimer(ResourceClass::Cache, None)?;
    drop(cache);
    assert_eq!(arbiter.snapshot()?.total_used_bytes(), 0);
    Ok(())
}

#[test]
fn concurrent_admission_never_crosses_global_or_class_hard_limits() -> Result<()> {
    const THREADS: usize = 16;
    let arbiter = EngineResourceArbiter::new(arbitration_config(100)?);
    let start = Arc::new(Barrier::new(THREADS + 1));
    let hold = Arc::new(Barrier::new(THREADS + 1));
    let (sender, receiver) = mpsc::channel();
    let mut workers = Vec::new();

    for _ in 0..THREADS {
        let arbiter = arbiter.clone();
        let start = Arc::clone(&start);
        let hold = Arc::clone(&hold);
        let sender = sender.clone();
        workers.push(thread::spawn(move || {
            start.wait();
            let admission = arbiter
                .reserve(
                    ResourceRequest::new(ResourceClass::Export, ResourceConsumer::Background, 20)
                        .expect("thread request is valid"),
                )
                .expect("concurrent arbiter state remains available");
            sender
                .send(matches!(&admission, ResourceAdmission::Granted(_)))
                .expect("result observer is alive");
            hold.wait();
            drop(admission);
        }));
    }
    drop(sender);
    start.wait();
    let granted_count = (0..THREADS)
        .map(|_| receiver.recv().expect("every worker reports admission"))
        .filter(|value| *value)
        .count();
    assert_eq!(granted_count, 5);
    let saturated = arbiter.snapshot()?;
    assert_eq!(saturated.total_used_bytes(), 100);
    assert_eq!(saturated.active_reservations(), 5);
    assert_eq!(saturated.class(ResourceClass::Export).used_bytes(), 100);
    hold.wait();
    for worker in workers {
        worker.join().expect("admission worker joins");
    }
    assert_eq!(arbiter.snapshot()?.total_used_bytes(), 0);
    Ok(())
}

#[test]
fn reclaim_callbacks_cannot_recursively_enter_the_serialized_admission_gate() -> Result<()> {
    let arbiter = EngineResourceArbiter::new(arbitration_config(100)?);
    let cache = granted(arbiter.reserve(ResourceRequest::new(
        ResourceClass::Cache,
        ResourceConsumer::Background,
        80,
    )?)?)
    .into_reservation();
    let reclaimer = Arc::new(ReentrantReclaimer::new(arbiter.clone(), cache));
    arbiter.set_reclaimer(ResourceClass::Cache, Some(reclaimer.clone()))?;

    let gpu = granted(arbiter.reserve(ResourceRequest::new(
        ResourceClass::GpuMemory,
        ResourceConsumer::Render,
        30,
    )?)?);
    assert_eq!(
        *reclaimer
            .observed
            .lock()
            .expect("test observation lock is not poisoned"),
        Some((ErrorCategory::Conflict, Recoverability::Retryable))
    );
    assert_eq!(arbiter.snapshot()?.total_used_bytes(), 30);
    arbiter.set_reclaimer(ResourceClass::Cache, None)?;
    drop(gpu);
    Ok(())
}
