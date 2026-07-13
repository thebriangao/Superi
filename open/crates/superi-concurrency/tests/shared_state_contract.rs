use std::sync::{mpsc, Arc};

use superi_concurrency::jobs::{JobKind, JobPriority, PriorityScheduler, ScheduledJob};
use superi_concurrency::shared::{
    DomainOwned, SharedSnapshot, SnapshotGeneration, SnapshotPublisher,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{ErrorCategory, Recoverability, Result};
use superi_core::ids::JobId;

#[derive(Debug, Eq, PartialEq)]
struct SchedulingSnapshot {
    queued_jobs: usize,
    next_job: JobId,
}

fn job(raw: u128, priority: JobPriority) -> ScheduledJob<&'static str> {
    ScheduledJob::new(JobId::from_raw(raw), JobKind::Frame, priority, "frame")
}

#[test]
fn mutable_owners_transfer_safely_but_enforce_their_execution_domain() -> Result<()> {
    fn assert_send<T: Send>() {}

    assert_send::<DomainOwned<String>>();
    assert_send::<SnapshotPublisher<String>>();

    let mut state = DomainOwned::new(ExecutionDomain::EngineControl, String::from("initial"));
    assert_eq!(state.domain(), ExecutionDomain::EngineControl);

    let unassigned = state.with(String::len).unwrap_err();
    assert_eq!(unassigned.category(), ErrorCategory::Conflict);
    assert_eq!(unassigned.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(
        unassigned.contexts().last().unwrap().component(),
        "superi-concurrency.shared"
    );
    assert_eq!(
        unassigned.contexts().last().unwrap().field("owner_domain"),
        Some("engine_control")
    );

    let ui = ExecutionDomain::Ui.enter_current()?;
    let wrong_domain = state.with(String::len).unwrap_err();
    assert_eq!(wrong_domain.category(), ErrorCategory::Conflict);
    assert_eq!(
        wrong_domain.contexts().first().unwrap().field("actual"),
        Some("ui")
    );
    drop(ui);

    let state = ExecutionDomain::EngineControl.spawn(move |_| {
        state.with_mut(|value| value.push_str(" state"))?;
        assert!(state.with(|value| value == "initial state")?);
        state.into_inner()
    })?;
    assert_eq!(state.join()?, "initial state");
    Ok(())
}

#[test]
fn real_scheduler_stays_single_owner_while_immutable_snapshots_cross_threads() -> Result<()> {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<SharedSnapshot<SchedulingSnapshot>>();

    let mut scheduler = DomainOwned::new(
        ExecutionDomain::EngineControl,
        PriorityScheduler::<&'static str>::new(),
    );
    let mut publisher = SnapshotPublisher::new(ExecutionDomain::EngineControl);
    assert_eq!(publisher.domain(), ExecutionDomain::EngineControl);
    let (first_tx, first_rx) = mpsc::sync_channel(1);

    let owner = ExecutionDomain::EngineControl.spawn(move |_| {
        scheduler.with_mut(|queue| queue.enqueue(job(1, JobPriority::Background)))??;
        scheduler.with_mut(|queue| queue.enqueue(job(2, JobPriority::Interactive)))??;

        let first_next = scheduler.with_mut(PriorityScheduler::next_job)?.unwrap();
        let first = publisher.publish(&Arc::new(SchedulingSnapshot {
            queued_jobs: scheduler.with(PriorityScheduler::len)?,
            next_job: first_next.id(),
        }))?;
        first_tx.send(first).unwrap();

        let second_next = scheduler.with_mut(PriorityScheduler::next_job)?.unwrap();
        let second = publisher.publish(&Arc::new(SchedulingSnapshot {
            queued_jobs: scheduler.with(PriorityScheduler::len)?,
            next_job: second_next.id(),
        }))?;
        Ok((scheduler.into_inner()?, second))
    })?;

    let first = first_rx.recv().unwrap();
    let first_clone = first.clone();
    let reader = std::thread::spawn(move || {
        let ui = ExecutionDomain::Ui.enter_current().unwrap();
        assert_eq!(first_clone.generation(), SnapshotGeneration::new(1));
        assert_eq!(first_clone.generation().get(), 1);
        assert_eq!(first_clone.source_domain(), ExecutionDomain::EngineControl);
        assert_eq!(
            first_clone.value(),
            &SchedulingSnapshot {
                queued_jobs: 1,
                next_job: JobId::from_raw(2),
            }
        );
        drop(ui);
        first_clone
    });

    let (scheduler, second) = owner.join()?;
    let returned_first = reader.join().unwrap();
    assert!(first.ptr_eq(&returned_first));
    assert_eq!(scheduler.len(), 0);
    assert_eq!(second.generation(), SnapshotGeneration::new(2));
    assert_eq!(
        second.value(),
        &SchedulingSnapshot {
            queued_jobs: 0,
            next_job: JobId::from_raw(1),
        }
    );
    assert_eq!(
        first.value(),
        &SchedulingSnapshot {
            queued_jobs: 1,
            next_job: JobId::from_raw(2),
        }
    );
    assert!(!first.ptr_eq(&second));
    Ok(())
}

#[test]
fn publication_requires_its_owner_and_rejects_the_audio_callback() -> Result<()> {
    let value = Arc::new(String::from("snapshot"));
    let mut engine = SnapshotPublisher::new(ExecutionDomain::EngineControl);
    let unassigned = engine.publish(&value).unwrap_err();
    assert_eq!(unassigned.category(), ErrorCategory::Conflict);
    assert_eq!(engine.current_generation(), SnapshotGeneration::INITIAL);

    let audio = ExecutionDomain::Audio.enter_current()?;
    let mut audio_publisher = SnapshotPublisher::new(ExecutionDomain::Audio);
    let forbidden = audio_publisher.publish(&value).unwrap_err();
    assert_eq!(forbidden.category(), ErrorCategory::Conflict);
    assert_eq!(forbidden.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(
        forbidden.contexts().last().unwrap().field("owner_domain"),
        Some("audio")
    );
    assert_eq!(
        audio_publisher.current_generation(),
        SnapshotGeneration::INITIAL
    );
    drop(audio);
    Ok(())
}

#[test]
fn generation_exhaustion_fails_before_publishing_ambiguous_state() -> Result<()> {
    let mut publisher = SnapshotPublisher::from_generation(
        ExecutionDomain::EngineControl,
        SnapshotGeneration::new(u64::MAX),
    );
    let owner = ExecutionDomain::EngineControl.enter_current()?;
    let error = publisher.publish(&Arc::new(7_u64)).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
    assert_eq!(error.recoverability(), Recoverability::Terminal);
    assert_eq!(
        error.contexts().last().unwrap().field("generation"),
        Some("18446744073709551615")
    );
    assert_eq!(
        publisher.current_generation(),
        SnapshotGeneration::new(u64::MAX)
    );
    drop(owner);
    Ok(())
}
