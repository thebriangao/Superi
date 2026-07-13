use std::collections::BTreeSet;
use std::time::Duration;

use superi_concurrency::lifecycle::{
    LifecycleCoordinator, LifecycleParticipant, LifecyclePhase, LifecycleSignal,
    LifecycleSignalSnapshot,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{Error, ErrorCategory, Recoverability, Result as SuperiResult};

fn pending(snapshot: &superi_concurrency::lifecycle::LifecycleSnapshot) -> Vec<&str> {
    snapshot
        .pending_participants()
        .iter()
        .map(String::as_str)
        .collect()
}

fn acknowledge_all(
    participants: &[&LifecycleParticipant],
    observed: LifecycleSignalSnapshot,
) -> SuperiResult<()> {
    for participant in participants {
        participant.acknowledge(observed)?;
    }
    Ok(())
}

fn running_pair() -> SuperiResult<(
    LifecycleCoordinator,
    LifecycleParticipant,
    LifecycleParticipant,
)> {
    let coordinator = LifecycleCoordinator::new(["playback", "render"])?;
    let playback = coordinator.participant("playback")?;
    let render = coordinator.participant("render")?;
    let startup = coordinator.signal().load();
    acknowledge_all(&[&playback, &render], startup)?;
    assert_eq!(coordinator.snapshot().phase(), LifecyclePhase::Running);
    Ok((coordinator, playback, render))
}

#[test]
fn phases_have_stable_identity_and_acknowledgement_policy() {
    assert_eq!(
        LifecyclePhase::ALL
            .iter()
            .map(|phase| phase.code())
            .collect::<Vec<_>>(),
        [
            "starting",
            "running",
            "pausing",
            "paused",
            "resuming",
            "preparing_sleep",
            "sleeping",
            "waking",
            "stopping",
            "stopped",
            "failed",
        ]
    );
    assert_eq!(
        LifecyclePhase::ALL
            .iter()
            .map(|phase| phase.code())
            .collect::<BTreeSet<_>>()
            .len(),
        LifecyclePhase::ALL.len()
    );
    for phase in [
        LifecyclePhase::Starting,
        LifecyclePhase::Pausing,
        LifecyclePhase::Resuming,
        LifecyclePhase::PreparingSleep,
        LifecyclePhase::Waking,
        LifecyclePhase::Stopping,
    ] {
        assert!(phase.requires_acknowledgement());
    }
}

#[test]
fn empty_participant_sets_complete_every_transition_immediately() -> SuperiResult<()> {
    let coordinator = LifecycleCoordinator::new(std::iter::empty::<&str>())?;
    assert_eq!(coordinator.snapshot().phase(), LifecyclePhase::Running);
    assert_eq!(coordinator.request_pause()?.phase(), LifecyclePhase::Paused);
    assert_eq!(
        coordinator.request_resume()?.phase(),
        LifecyclePhase::Running
    );
    assert_eq!(
        coordinator.request_sleep()?.phase(),
        LifecyclePhase::Sleeping
    );
    assert_eq!(coordinator.request_wake()?.phase(), LifecyclePhase::Running);
    assert_eq!(
        coordinator.request_shutdown()?.phase(),
        LifecyclePhase::Stopped
    );
    assert_eq!(
        coordinator.request_restart()?.phase(),
        LifecyclePhase::Running
    );
    Ok(())
}

#[test]
fn lifecycle_transitions_are_explicit_acknowledged_and_restartable() -> SuperiResult<()> {
    let coordinator = LifecycleCoordinator::new(["render", "playback"])?;
    let playback = coordinator.participant("playback")?;
    let render = coordinator.participant("render")?;

    let startup = coordinator.snapshot();
    assert_eq!(startup.phase(), LifecyclePhase::Starting);
    assert_eq!(startup.revision(), 0);
    assert_eq!(pending(&startup), ["playback", "render"]);
    assert_eq!(
        coordinator.signal().load().phase(),
        LifecyclePhase::Starting
    );

    let startup_signal = coordinator.signal().load();
    playback.acknowledge(startup_signal)?;
    assert_eq!(pending(&coordinator.snapshot()), ["render"]);
    render.acknowledge(startup_signal)?;
    assert_eq!(coordinator.snapshot().phase(), LifecyclePhase::Running);

    let pausing = coordinator.request_pause()?;
    assert_eq!(pausing.phase(), LifecyclePhase::Pausing);
    assert!(pausing.revision() > startup.revision());
    let stale = playback.acknowledge(startup_signal).unwrap_err();
    assert_eq!(stale.category(), ErrorCategory::Conflict);
    assert_eq!(stale.recoverability(), Recoverability::Retryable);
    acknowledge_all(&[&playback, &render], coordinator.signal().load())?;
    assert_eq!(coordinator.snapshot().phase(), LifecyclePhase::Paused);

    assert_eq!(
        coordinator.request_resume()?.phase(),
        LifecyclePhase::Resuming
    );
    acknowledge_all(&[&playback, &render], coordinator.signal().load())?;
    assert_eq!(coordinator.snapshot().phase(), LifecyclePhase::Running);

    assert_eq!(
        coordinator.request_sleep()?.phase(),
        LifecyclePhase::PreparingSleep
    );
    acknowledge_all(&[&playback, &render], coordinator.signal().load())?;
    assert_eq!(coordinator.snapshot().phase(), LifecyclePhase::Sleeping);

    assert_eq!(coordinator.request_wake()?.phase(), LifecyclePhase::Waking);
    acknowledge_all(&[&playback, &render], coordinator.signal().load())?;
    assert_eq!(coordinator.snapshot().phase(), LifecyclePhase::Running);

    assert_eq!(
        coordinator.request_shutdown()?.phase(),
        LifecyclePhase::Stopping
    );
    acknowledge_all(&[&playback, &render], coordinator.signal().load())?;
    assert_eq!(coordinator.snapshot().phase(), LifecyclePhase::Stopped);

    assert_eq!(
        coordinator.request_restart()?.phase(),
        LifecyclePhase::Starting
    );
    acknowledge_all(&[&playback, &render], coordinator.signal().load())?;
    let restarted = coordinator.snapshot();
    assert_eq!(restarted.phase(), LifecyclePhase::Running);
    assert!(restarted.failure().is_none());
    Ok(())
}

#[test]
fn wake_restores_paused_intent_and_invalidates_sleep_acknowledgements() -> SuperiResult<()> {
    let (coordinator, playback, render) = running_pair()?;

    coordinator.request_pause()?;
    acknowledge_all(&[&playback, &render], coordinator.signal().load())?;
    assert_eq!(coordinator.snapshot().phase(), LifecyclePhase::Paused);

    coordinator.request_sleep()?;
    let sleeping = coordinator.signal().load();
    playback.acknowledge(sleeping)?;
    assert_eq!(pending(&coordinator.snapshot()), ["render"]);

    let waking = coordinator.request_wake()?;
    assert_eq!(waking.phase(), LifecyclePhase::Waking);
    assert_eq!(pending(&waking), ["playback", "render"]);
    assert_eq!(
        render.acknowledge(sleeping).unwrap_err().category(),
        ErrorCategory::Conflict
    );
    acknowledge_all(&[&playback, &render], coordinator.signal().load())?;
    assert_eq!(coordinator.snapshot().phase(), LifecyclePhase::Paused);

    assert_eq!(coordinator.request_wake()?.phase(), LifecyclePhase::Waking);
    acknowledge_all(&[&playback, &render], coordinator.signal().load())?;
    assert_eq!(coordinator.snapshot().phase(), LifecyclePhase::Paused);
    Ok(())
}

#[test]
fn failures_remain_classified_until_an_orderly_shutdown_and_restart() -> SuperiResult<()> {
    let (coordinator, playback, render) = running_pair()?;
    playback.fail(Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        "audio device disappeared",
    ))?;

    let failed = coordinator.snapshot();
    assert_eq!(failed.phase(), LifecyclePhase::Failed);
    let failure = failed.failure().unwrap();
    assert_eq!(failure.participant(), "playback");
    assert_eq!(failure.error().category(), ErrorCategory::Unavailable);
    assert_eq!(failure.error().recoverability(), Recoverability::Retryable);
    assert_eq!(failure.error().message(), "audio device disappeared");

    let restart_error = coordinator.request_restart().unwrap_err();
    assert_eq!(restart_error.category(), ErrorCategory::Conflict);

    coordinator.request_shutdown()?;
    acknowledge_all(&[&playback, &render], coordinator.signal().load())?;
    let stopped = coordinator.snapshot();
    assert_eq!(stopped.phase(), LifecyclePhase::Stopped);
    assert!(stopped.failure().is_some());

    coordinator.request_restart()?;
    assert!(coordinator.snapshot().failure().is_none());
    acknowledge_all(&[&playback, &render], coordinator.signal().load())?;
    assert_eq!(coordinator.snapshot().phase(), LifecyclePhase::Running);
    Ok(())
}

#[test]
fn atomic_signals_serve_latency_sensitive_domains_without_blocking() -> SuperiResult<()> {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<LifecycleSignal>();
    assert_send_sync::<LifecycleCoordinator>();
    assert_send_sync::<LifecycleParticipant>();

    let (coordinator, _playback, _render) = running_pair()?;
    let signal = coordinator.signal();
    let initial = signal.load();
    assert_eq!(initial.phase(), LifecyclePhase::Running);

    let audio = std::thread::spawn(move || {
        let _audio = ExecutionDomain::Audio.enter_current().unwrap();
        signal.load()
    });
    assert_eq!(audio.join().unwrap(), initial);

    coordinator.request_pause()?;
    let changed = coordinator.signal().load();
    assert_eq!(changed.phase(), LifecyclePhase::Pausing);
    assert!(changed.revision() > initial.revision());
    Ok(())
}

#[test]
fn critical_wake_without_prior_sleep_revalidates_active_participants() -> SuperiResult<()> {
    let (coordinator, playback, render) = running_pair()?;
    let waking = coordinator.request_wake()?;
    assert_eq!(waking.phase(), LifecyclePhase::Waking);
    assert_eq!(pending(&waking), ["playback", "render"]);

    assert_eq!(coordinator.request_wake()?.revision(), waking.revision());
    acknowledge_all(&[&playback, &render], coordinator.signal().load())?;
    assert_eq!(coordinator.snapshot().phase(), LifecyclePhase::Running);
    Ok(())
}

#[test]
fn blocking_observation_is_restricted_and_wakes_on_a_real_transition() -> SuperiResult<()> {
    let (coordinator, _playback, _render) = running_pair()?;
    let revision = coordinator.snapshot().revision();

    let ui = ExecutionDomain::Ui.enter_current()?;
    let error = coordinator
        .wait_for_change(revision, Duration::from_millis(1))
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    drop(ui);

    let waiting = coordinator.clone();
    let waiter = ExecutionDomain::BackgroundJob
        .spawn(move |_| waiting.wait_for_change(revision, Duration::from_secs(1)))?;
    coordinator.request_pause()?;
    let changed = waiter.join()?.expect("transition should wake the observer");
    assert_eq!(changed.phase(), LifecyclePhase::Pausing);

    let timed_out = coordinator.wait_for_change(changed.revision(), Duration::from_millis(1))?;
    assert!(timed_out.is_none());
    Ok(())
}

#[test]
fn participant_registration_and_invalid_transitions_are_diagnostic() -> SuperiResult<()> {
    assert_eq!(
        LifecycleCoordinator::new(["render", "render"])
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        LifecycleCoordinator::new([""]).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );

    let (coordinator, _playback, _render) = running_pair()?;
    assert_eq!(
        coordinator.participant("missing").unwrap_err().category(),
        ErrorCategory::NotFound
    );
    assert_eq!(
        coordinator.request_restart().unwrap_err().category(),
        ErrorCategory::Conflict
    );
    Ok(())
}
