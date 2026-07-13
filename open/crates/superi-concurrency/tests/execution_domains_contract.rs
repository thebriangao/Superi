use std::collections::{BTreeSet, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;

use superi_concurrency::threads::{current_execution_domain, ExecutionDomain};
use superi_core::error::{ErrorCategory, Recoverability};
use superi_gpu::device::{
    AdapterSelection, DeviceRequest, GpuDevice, GpuInstance, InstanceOptions,
};
use superi_gpu::submission::GpuSubmissionQueue;

fn test_device() -> Option<GpuDevice> {
    let instance = GpuInstance::new(InstanceOptions::default()).ok()?;
    let adapter = instance
        .enumerate_adapters()
        .select(&AdapterSelection::default())
        .ok()?;
    pollster::block_on(
        adapter.create_device(&DeviceRequest::default().with_label("execution domain contract")),
    )
    .ok()
}

#[test]
fn domains_have_stable_identity_and_explicit_execution_policy() {
    assert_eq!(
        ExecutionDomain::ALL,
        &[
            ExecutionDomain::Ui,
            ExecutionDomain::EngineControl,
            ExecutionDomain::Playback,
            ExecutionDomain::Render,
            ExecutionDomain::Audio,
            ExecutionDomain::BackgroundJob,
            ExecutionDomain::GpuSubmission,
        ]
    );

    let codes = ExecutionDomain::ALL
        .iter()
        .map(|domain| domain.code())
        .collect::<BTreeSet<_>>();
    assert_eq!(codes.len(), ExecutionDomain::ALL.len());
    assert_eq!(ExecutionDomain::Ui.code(), "ui");
    assert_eq!(ExecutionDomain::EngineControl.code(), "engine_control");
    assert_eq!(ExecutionDomain::Playback.code(), "playback");
    assert_eq!(ExecutionDomain::Render.code(), "render");
    assert_eq!(ExecutionDomain::Audio.code(), "audio");
    assert_eq!(ExecutionDomain::BackgroundJob.code(), "background_job");
    assert_eq!(ExecutionDomain::GpuSubmission.code(), "gpu_submission");

    let ui = ExecutionDomain::Ui.policy();
    assert!(ui.is_platform_owned());
    assert!(!ui.allows_blocking());
    assert!(ui.allows_allocation());

    let audio = ExecutionDomain::Audio.policy();
    assert!(audio.is_platform_owned());
    assert!(!audio.allows_blocking());
    assert!(!audio.allows_allocation());

    for domain in [
        ExecutionDomain::EngineControl,
        ExecutionDomain::Playback,
        ExecutionDomain::Render,
        ExecutionDomain::BackgroundJob,
        ExecutionDomain::GpuSubmission,
    ] {
        assert!(!domain.policy().is_platform_owned());
    }
    assert!(!ExecutionDomain::EngineControl.policy().allows_blocking());
    assert!(!ExecutionDomain::Playback.policy().allows_blocking());
    assert!(ExecutionDomain::Render.policy().allows_blocking());
    assert!(ExecutionDomain::BackgroundJob.policy().allows_blocking());
    assert!(ExecutionDomain::GpuSubmission.policy().allows_blocking());
}

#[test]
fn platform_owned_domains_attach_to_exactly_one_current_thread_scope() {
    assert_eq!(current_execution_domain(), None);
    let ui = ExecutionDomain::Ui.enter_current().unwrap();

    assert_eq!(ui.domain(), ExecutionDomain::Ui);
    assert_eq!(ui.thread_id(), Some(thread::current().id()));
    assert_eq!(current_execution_domain(), Some(ExecutionDomain::Ui));
    ExecutionDomain::Ui.require_current().unwrap();

    let wrong = ExecutionDomain::Audio.require_current().unwrap_err();
    assert_eq!(wrong.category(), ErrorCategory::Conflict);
    assert_eq!(wrong.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(
        wrong.contexts()[0].component(),
        "superi-concurrency.threads"
    );
    assert_eq!(wrong.contexts()[0].field("expected"), Some("audio"));
    assert_eq!(wrong.contexts()[0].field("actual"), Some("ui"));

    let nested = ExecutionDomain::Audio.enter_current().unwrap_err();
    assert_eq!(nested.category(), ErrorCategory::Conflict);
    assert_eq!(current_execution_domain(), Some(ExecutionDomain::Ui));

    drop(ui);
    assert_eq!(current_execution_domain(), None);

    let audio = ExecutionDomain::Audio.enter_current().unwrap();
    ExecutionDomain::Audio.require_current().unwrap();
    drop(audio);
    assert_eq!(current_execution_domain(), None);
}

#[test]
fn managed_domains_run_on_named_owned_threads_and_surface_failures() {
    for domain in [
        ExecutionDomain::EngineControl,
        ExecutionDomain::Playback,
        ExecutionDomain::Render,
        ExecutionDomain::BackgroundJob,
        ExecutionDomain::GpuSubmission,
    ] {
        let worker = domain
            .spawn(move |guard| {
                domain.require_current()?;
                Ok((
                    guard.domain(),
                    guard.thread_id().unwrap(),
                    current_execution_domain(),
                    thread::current().name().map(str::to_owned),
                ))
            })
            .unwrap();
        assert_eq!(worker.domain(), domain);
        assert_eq!(worker.thread_name(), domain.thread_name());
        let worker_id = worker.thread_id();
        assert_ne!(worker_id, thread::current().id());

        let (actual, reported_id, current, name) = worker.join().unwrap();
        assert_eq!(actual, domain);
        assert_eq!(reported_id, worker_id);
        assert_eq!(current, Some(domain));
        assert_eq!(name.as_deref(), Some(domain.thread_name()));
    }

    for platform_owned in [ExecutionDomain::Ui, ExecutionDomain::Audio] {
        let error = match platform_owned.spawn(|_| Ok(())) {
            Ok(_) => panic!("platform-owned domains must not spawn managed workers"),
            Err(error) => error,
        };
        assert_eq!(error.category(), ErrorCategory::InvalidInput);
        assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
        assert_eq!(
            error.contexts()[0].field("domain"),
            Some(platform_owned.code())
        );
    }

    let panicking = ExecutionDomain::BackgroundJob
        .spawn(|_| -> superi_core::error::Result<()> {
            panic!("synthetic worker failure");
        })
        .unwrap();
    let error = panicking.join().unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Internal);
    assert_eq!(error.recoverability(), Recoverability::Terminal);
    assert_eq!(error.contexts()[0].field("domain"), Some("background_job"));
}

#[test]
fn gpu_submission_domain_hosts_the_real_non_send_queue_owner() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping execution domain GPU integration");
        return;
    };

    ExecutionDomain::GpuSubmission
        .spawn(move |_| {
            ExecutionDomain::GpuSubmission.require_current()?;
            let submissions = GpuSubmissionQueue::new(&device)?;
            let fence = submissions.submit(std::iter::empty(), submissions.resources())?;
            let progress = submissions.wait(&fence)?;
            assert_eq!(progress.last_submitted(), fence.value());
            assert_eq!(progress.last_retired(), fence.value());
            Ok(())
        })
        .unwrap()
        .join()
        .unwrap();
}

#[test]
fn latency_sensitive_domains_progress_while_competing_domains_are_blocked() {
    let entered = Arc::new(Barrier::new(3));
    let release = Arc::new(Barrier::new(3));

    let render = {
        let entered = Arc::clone(&entered);
        let release = Arc::clone(&release);
        ExecutionDomain::Render
            .spawn(move |_| {
                entered.wait();
                release.wait();
                ExecutionDomain::Render.require_current()
            })
            .unwrap()
    };
    let background = {
        let entered = Arc::clone(&entered);
        let release = Arc::clone(&release);
        ExecutionDomain::BackgroundJob
            .spawn(move |_| {
                entered.wait();
                release.wait();
                ExecutionDomain::BackgroundJob.require_current()
            })
            .unwrap()
    };

    entered.wait();

    let playback_ran = Arc::new(AtomicBool::new(false));
    let playback_ran_in_domain = Arc::clone(&playback_ran);
    let playback = {
        ExecutionDomain::Playback
            .spawn(move |_| {
                ExecutionDomain::Playback.require_current()?;
                playback_ran_in_domain.store(true, Ordering::Release);
                Ok(())
            })
            .unwrap()
    };
    let playback_thread_id = playback.thread_id();
    let gpu_ran = Arc::new(AtomicBool::new(false));
    let gpu_ran_in_domain = Arc::clone(&gpu_ran);
    let gpu = {
        ExecutionDomain::GpuSubmission
            .spawn(move |_| {
                ExecutionDomain::GpuSubmission.require_current()?;
                gpu_ran_in_domain.store(true, Ordering::Release);
                Ok(())
            })
            .unwrap()
    };
    let gpu_thread_id = gpu.thread_id();
    let audio_ran = Arc::new(AtomicBool::new(false));
    let audio_ran_in_callback = Arc::clone(&audio_ran);
    let audio = thread::spawn(move || {
        let _audio = ExecutionDomain::Audio.enter_current().unwrap();
        ExecutionDomain::Audio.require_current().unwrap();
        audio_ran_in_callback.store(true, Ordering::Release);
    });
    let audio_thread_id = audio.thread().id();

    let ui = ExecutionDomain::Ui.enter_current().unwrap();
    ExecutionDomain::Ui.require_current().unwrap();
    let ui_thread_id = ui.thread_id().unwrap();
    drop(ui);

    playback.join().unwrap();
    gpu.join().unwrap();
    audio.join().unwrap();
    assert!(playback_ran.load(Ordering::Acquire));
    assert!(gpu_ran.load(Ordering::Acquire));
    assert!(audio_ran.load(Ordering::Acquire));
    assert_eq!(
        HashSet::from([
            ui_thread_id,
            playback_thread_id,
            gpu_thread_id,
            audio_thread_id,
        ])
        .len(),
        4
    );

    release.wait();
    render.join().unwrap();
    background.join().unwrap();
}
