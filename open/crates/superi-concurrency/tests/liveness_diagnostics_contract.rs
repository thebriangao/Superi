use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use superi_concurrency::diagnostics::{
    LivenessActivity, LivenessActor, LivenessMonitor, LivenessRegistration, WaitResource,
};
use superi_concurrency::jobs::{JobKind, JobPriority};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::diagnostics::{DiagnosticSeverity, TraceValue};
use superi_core::error::{ErrorCategory, Result};
use superi_core::ids::JobId;

fn instant_after(base: Instant, duration: Duration) -> Instant {
    base.checked_add(duration).unwrap()
}

fn job(raw: u128) -> LivenessActor {
    LivenessActor::job(JobId::from_raw(raw))
}

fn job_registration(raw: u128, priority: JobPriority) -> LivenessRegistration {
    LivenessRegistration::job(
        JobId::from_raw(raw),
        JobKind::Frame,
        priority,
        Duration::from_millis(10),
    )
}

#[test]
fn actors_resources_and_registrations_are_typed_and_validated() {
    let domain = LivenessActor::domain(ExecutionDomain::Playback);
    let scheduled = job(7);
    assert_eq!(domain.kind_code(), "execution_domain");
    assert_eq!(domain.to_string(), "domain:playback");
    assert_eq!(scheduled.kind_code(), "job");
    assert_eq!(scheduled.to_string(), JobId::from_raw(7).to_string());
    assert!(domain < scheduled);

    let resource = WaitResource::new("gpu.queue").unwrap();
    assert_eq!(resource.as_str(), "gpu.queue");
    for invalid in ["", "GPU.queue", ".gpu", "gpu..queue", "gpu queue"] {
        let error = WaitResource::new(invalid).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::InvalidInput);
    }

    let monitor = LivenessMonitor::new();
    let playback = monitor
        .register(LivenessRegistration::domain(
            ExecutionDomain::Playback,
            Duration::from_millis(5),
        ))
        .unwrap();
    assert_eq!(playback.actor(), domain);
    assert_eq!(playback.activity(), LivenessActivity::Idle);

    let duplicate = monitor
        .register(LivenessRegistration::domain(
            ExecutionDomain::Playback,
            Duration::from_millis(5),
        ))
        .unwrap_err();
    assert_eq!(duplicate.category(), ErrorCategory::Conflict);

    let invalid_threshold = LivenessRegistration::domain(ExecutionDomain::Ui, Duration::ZERO)
        .validate()
        .unwrap_err();
    assert_eq!(invalid_threshold.category(), ErrorCategory::InvalidInput);

    monitor
        .register_resource(resource.clone(), playback.actor())
        .unwrap();
    assert_eq!(
        monitor
            .register_resource(resource, playback.actor())
            .unwrap_err()
            .category(),
        ErrorCategory::Conflict
    );
}

#[test]
fn latency_sensitive_progress_reporting_is_atomic_and_observer_sampled() -> Result<()> {
    let monitor = LivenessMonitor::new();
    let ui = monitor.register(LivenessRegistration::domain(
        ExecutionDomain::Ui,
        Duration::from_millis(10),
    ))?;
    let audio = monitor.register(LivenessRegistration::domain(
        ExecutionDomain::Audio,
        Duration::from_millis(10),
    ))?;

    let guard = ExecutionDomain::Ui.enter_current()?;
    ui.mark_runnable();
    assert_eq!(ui.activity(), LivenessActivity::Runnable);
    assert_eq!(ui.record_progress()?, 1);
    assert_eq!(ui.revision(), 1);
    ui.mark_idle();
    assert_eq!(ui.activity(), LivenessActivity::Idle);
    drop(guard);

    let guard = ExecutionDomain::Audio.enter_current()?;
    audio.mark_runnable();
    assert_eq!(audio.record_progress()?, 1);
    audio.mark_idle();
    drop(guard);

    let snapshot = monitor.snapshot();
    assert_eq!(snapshot.actors().len(), 2);
    assert!(snapshot.starvations().is_empty());
    Ok(())
}

#[test]
fn progress_revisions_remain_monotonic_under_contention() -> Result<()> {
    let monitor = LivenessMonitor::new();
    let probe = Arc::new(monitor.register(job_registration(1, JobPriority::Background))?);
    probe.mark_runnable();
    let workers = (0..8)
        .map(|_| {
            let probe = Arc::clone(&probe);
            thread::spawn(move || -> Result<()> {
                for _ in 0..1_000 {
                    probe.record_progress()?;
                }
                Ok(())
            })
        })
        .collect::<Vec<_>>();
    for worker in workers {
        worker.join().unwrap()?;
    }
    assert_eq!(probe.revision(), 8_000);
    assert_eq!(monitor.snapshot().actors()[0].progress_revision(), 8_000);
    Ok(())
}

#[test]
fn starvation_requires_runnable_or_waiting_state_and_recovers_on_progress() -> Result<()> {
    let base = Instant::now();
    let monitor = LivenessMonitor::new();
    let probe = monitor.register_at(job_registration(1, JobPriority::Interactive), base)?;
    probe.mark_runnable();

    assert!(monitor.snapshot_at(base).starvations().is_empty());
    assert!(monitor
        .snapshot_at(instant_after(base, Duration::from_millis(9)))
        .starvations()
        .is_empty());

    let threshold = monitor.snapshot_at(instant_after(base, Duration::from_millis(10)));
    let starvation = &threshold.starvations()[0];
    assert_eq!(starvation.actor(), job(1));
    assert_eq!(starvation.activity(), LivenessActivity::Runnable);
    assert_eq!(starvation.stalled_for(), Duration::from_millis(10));
    assert_eq!(starvation.threshold(), Duration::from_millis(10));
    assert_eq!(starvation.priority(), Some(JobPriority::Interactive));
    assert_eq!(starvation.resource(), None);

    assert_eq!(probe.record_progress()?, 1);
    assert!(monitor
        .snapshot_at(instant_after(base, Duration::from_millis(20)))
        .starvations()
        .is_empty());
    probe.mark_idle();
    assert!(monitor
        .snapshot_at(instant_after(base, Duration::from_secs(1)))
        .starvations()
        .is_empty());
    Ok(())
}

#[test]
fn owned_waits_form_canonical_deadlock_cycles_and_raii_clears_them() -> Result<()> {
    let base = Instant::now();
    let monitor = LivenessMonitor::new();
    let first = monitor.register_at(job_registration(1, JobPriority::Playback), base)?;
    let second = monitor.register_at(job_registration(2, JobPriority::Export), base)?;
    first.mark_runnable();
    second.mark_runnable();

    let first_resource = WaitResource::new("job.first.output")?;
    let second_resource = WaitResource::new("job.second.output")?;
    monitor.register_resource(first_resource.clone(), first.actor())?;
    monitor.register_resource(second_resource.clone(), second.actor())?;

    let first_wait = monitor.begin_wait_at(first.actor(), second_resource.clone(), base)?;
    let second_wait = monitor.begin_wait_at(
        second.actor(),
        first_resource.clone(),
        instant_after(base, Duration::from_millis(1)),
    )?;
    assert_eq!(first.activity(), LivenessActivity::Waiting);
    assert_eq!(second.activity(), LivenessActivity::Waiting);

    let snapshot = monitor.snapshot_at(instant_after(base, Duration::from_millis(12)));
    assert_eq!(snapshot.waits().len(), 2);
    assert_eq!(snapshot.starvations().len(), 2);
    assert_eq!(snapshot.deadlocks().len(), 1);
    let cycle = snapshot.deadlocks()[0].edges();
    assert_eq!(cycle.len(), 2);
    assert_eq!(cycle[0].waiter(), first.actor());
    assert_eq!(cycle[0].resource(), &second_resource);
    assert_eq!(cycle[0].owner(), second.actor());
    assert_eq!(cycle[1].waiter(), second.actor());
    assert_eq!(cycle[1].resource(), &first_resource);
    assert_eq!(cycle[1].owner(), first.actor());

    drop(second_wait);
    assert_eq!(second.activity(), LivenessActivity::Runnable);
    assert!(monitor.snapshot().deadlocks().is_empty());
    drop(first_wait);
    assert_eq!(first.activity(), LivenessActivity::Runnable);
    assert!(monitor.snapshot().waits().is_empty());
    Ok(())
}

#[test]
fn unowned_resources_do_not_produce_false_deadlocks_and_owner_transfer_is_visible() -> Result<()> {
    let monitor = LivenessMonitor::new();
    let first = monitor.register(job_registration(1, JobPriority::Playback))?;
    let second = monitor.register(job_registration(2, JobPriority::Background))?;
    let resource = WaitResource::new("decode.packet")?;
    monitor.register_resource(resource.clone(), second.actor())?;

    let wait = monitor.begin_wait(first.actor(), resource.clone())?;
    let owned = monitor.snapshot();
    assert_eq!(owned.waits()[0].owner(), Some(second.actor()));
    assert!(owned.deadlocks().is_empty());

    monitor.release_resource(&resource)?;
    let unowned = monitor.snapshot();
    assert_eq!(unowned.waits()[0].owner(), None);
    assert!(unowned.deadlocks().is_empty());

    monitor.set_resource_owner(&resource, first.actor())?;
    let self_cycle = monitor.snapshot();
    assert_eq!(self_cycle.deadlocks().len(), 1);
    assert_eq!(self_cycle.deadlocks()[0].edges().len(), 1);
    assert_eq!(self_cycle.deadlocks()[0].edges()[0].waiter(), first.actor());
    drop(wait);
    Ok(())
}

#[test]
fn findings_project_to_stable_structured_diagnostic_events() -> Result<()> {
    let base = Instant::now();
    let monitor = LivenessMonitor::new();
    let first = monitor.register_at(job_registration(1, JobPriority::Interactive), base)?;
    let second = monitor.register_at(job_registration(2, JobPriority::Playback), base)?;
    first.mark_runnable();
    second.mark_runnable();
    let _ = monitor.snapshot_at(base);

    let first_resource = WaitResource::new("resource.first")?;
    let second_resource = WaitResource::new("resource.second")?;
    monitor.register_resource(first_resource.clone(), first.actor())?;
    monitor.register_resource(second_resource.clone(), second.actor())?;
    let _first_wait = monitor.begin_wait_at(first.actor(), second_resource, base)?;
    let _second_wait = monitor.begin_wait_at(second.actor(), first_resource, base)?;

    let snapshot = monitor.snapshot_at(instant_after(base, Duration::from_millis(10)));
    let events = snapshot.diagnostic_events()?;
    assert_eq!(events.len(), 3);
    assert_eq!(events[0].name(), "scheduler.starvation.detected");
    assert_eq!(events[0].severity(), DiagnosticSeverity::Warning);
    assert_eq!(events[1].name(), "scheduler.starvation.detected");
    assert_eq!(events[2].name(), "scheduler.deadlock.detected");
    assert_eq!(events[2].severity(), DiagnosticSeverity::Error);
    assert_eq!(
        events[2].field("cycle.actor_count").unwrap().value(),
        &TraceValue::Unsigned(2)
    );
    assert!(events[2].field("cycle.path").is_some());
    assert!(events
        .iter()
        .all(|event| event.component() == "superi-concurrency.diagnostics"));
    Ok(())
}

#[test]
fn public_diagnostic_state_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<LivenessMonitor>();
    assert_send_sync::<superi_concurrency::diagnostics::LivenessProbe>();
    assert_send_sync::<superi_concurrency::diagnostics::LivenessSnapshot>();
}
