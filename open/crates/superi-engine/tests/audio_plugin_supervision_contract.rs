use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use superi_audio::plugins::{
    AudioPluginBridgeStatus, AudioPluginFormat, AudioPluginIdentity, AudioPluginState,
    IsolatedAudioPluginProcessBridge,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::{ProjectId, TimelineId};
use superi_core::pixel::ChannelLayout;
use superi_core::time::{RationalTime, SampleTime, Timebase};
use superi_engine::audio_plugins::{
    audio_plugin_project_key, audio_plugin_project_record, audio_plugin_state_from_project_record,
    discover_vst3_bundles, AudioPluginDescriptor, AudioPluginInstanceId, AudioPluginLifecycle,
    AudioPluginSupervisor, AudioPluginWorkerAdapter, AudioPluginWorkerContract,
    AudioPluginWorkerIsolation, AudioPluginWorkerLauncher,
};
use superi_engine::extensions::{
    registrations_from_native_audio, ExtensionLifecycle, ExtensionRuntimeIdentity,
};
use superi_project::document::ProjectDocument;
use superi_project::extensions::ProjectExtensionLifecycle;
use superi_project::ProjectDatabase;
use superi_timeline::model::{EditorialProject, Timeline};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempTree {
    root: PathBuf,
}

impl TempTree {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "superi-audio-plugin-{label}-{}-{}",
            std::process::id(),
            NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir_all(&root).unwrap();
        Self { root }
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn bundle(&self, relative: &str) -> PathBuf {
        let bundle = self.root.join(relative);
        fs::create_dir_all(bundle.join("Contents")).unwrap();
        bundle
    }
}

impl Drop for TempTree {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
        let _ = fs::remove_file(&self.root);
    }
}

fn identity() -> AudioPluginIdentity {
    AudioPluginIdentity::new(
        AudioPluginFormat::Vst3,
        "Example Vendor",
        "6E33225254224A00AA69301AF318797D",
        "2.4.1",
    )
    .unwrap()
}

fn state(gain: u8) -> AudioPluginState {
    state_with_version("2.4.1", gain)
}

fn state_with_version(version: &str, gain: u8) -> AudioPluginState {
    AudioPluginState::new(
        AudioPluginIdentity::new(
            AudioPluginFormat::Vst3,
            "Example Vendor",
            "6E33225254224A00AA69301AF318797D",
            version,
        )
        .unwrap(),
        48_000,
        7,
        2,
        vec![gain, 0xfe],
        vec![0, gain],
    )
    .unwrap()
}

struct PassthroughBridge;

impl IsolatedAudioPluginProcessBridge for PassthroughBridge {
    fn fixed_transport_latency_samples(&self) -> usize {
        2
    }

    fn try_process(
        &mut self,
        _start_time: SampleTime,
        input: &[f32],
        output: &mut [f32],
    ) -> Result<AudioPluginBridgeStatus> {
        output.copy_from_slice(input);
        Ok(AudioPluginBridgeStatus::Produced)
    }
}

struct FixtureAdapter {
    activation_attempts: u32,
    checkpoint: AudioPluginState,
    always_fail_activation: bool,
}

impl AudioPluginWorkerAdapter for FixtureAdapter {
    fn contract(&self) -> AudioPluginWorkerContract {
        AudioPluginWorkerContract::new(
            AudioPluginWorkerIsolation::WorkerProcess,
            1,
            4 * 1024 * 1024,
            30_000,
            true,
            2,
        )
        .unwrap()
    }

    fn scan(&mut self) -> Result<AudioPluginDescriptor> {
        AudioPluginDescriptor::new(
            identity(),
            "Fixture Gain",
            7,
            [ChannelLayout::mono(), ChannelLayout::stereo()],
        )
    }

    fn activate(&mut self, state: Option<&AudioPluginState>) -> Result<()> {
        self.activation_attempts += 1;
        if self.always_fail_activation || self.activation_attempts == 1 {
            return Err(Error::new(
                ErrorCategory::Timeout,
                Recoverability::Retryable,
                "fixture worker missed its activation deadline",
            )
            .with_context(ErrorContext::new(
                "superi-engine-test.audio-plugin",
                "activate",
            )));
        }
        if let Some(state) = state {
            self.checkpoint = AudioPluginState::new(
                identity(),
                state.sample_rate(),
                state.native_latency_samples(),
                state.transport_latency_samples(),
                state.component_state().to_vec(),
                state.controller_state().to_vec(),
            )?;
        }
        Ok(())
    }

    fn capture_state(&mut self) -> Result<AudioPluginState> {
        Ok(self.checkpoint.clone())
    }

    fn prepare_process_bridge(
        &mut self,
        _sample_rate: u32,
        _layout: &ChannelLayout,
        _maximum_frames: usize,
    ) -> Result<Box<dyn IsolatedAudioPluginProcessBridge>> {
        Ok(Box::new(PassthroughBridge))
    }

    fn deactivate(&mut self) -> Result<()> {
        Ok(())
    }

    fn restart_worker(&mut self) -> Result<()> {
        Ok(())
    }
}

struct FixtureLauncher;

impl AudioPluginWorkerLauncher for FixtureLauncher {
    fn launch(
        &mut self,
        _candidate: &superi_engine::audio_plugins::AudioPluginCandidate,
    ) -> Result<Box<dyn AudioPluginWorkerAdapter>> {
        Ok(Box::new(FixtureAdapter {
            activation_attempts: 0,
            checkpoint: state(1),
            always_fail_activation: false,
        }))
    }
}

struct AlwaysFailLauncher;

impl AudioPluginWorkerLauncher for AlwaysFailLauncher {
    fn launch(
        &mut self,
        _candidate: &superi_engine::audio_plugins::AudioPluginCandidate,
    ) -> Result<Box<dyn AudioPluginWorkerAdapter>> {
        Ok(Box::new(FixtureAdapter {
            activation_attempts: 0,
            checkpoint: state(1),
            always_fail_activation: true,
        }))
    }
}

fn document() -> ProjectDocument {
    let project_id = ProjectId::from_raw(0xc014_0001);
    let timeline_id = TimelineId::from_raw(0xc014_0002);
    let timebase = Timebase::integer(24).unwrap();
    let timeline = Timeline::new(
        timeline_id,
        "audio plugin state",
        timebase,
        RationalTime::zero(timebase),
        vec![],
    );
    let project = EditorialProject::new(project_id, "audio plugin state", [], [timeline]).unwrap();
    ProjectDocument::new(project, timeline_id).unwrap()
}

#[test]
fn deterministic_scan_supervision_recovery_and_project_reopen_preserve_exact_state() {
    let _background = ExecutionDomain::BackgroundJob.enter_current().unwrap();
    let tree = TempTree::new("scan");
    tree.bundle("z/fixture.vst3");
    fs::create_dir_all(tree.path().join("a/broken.vst3")).unwrap();

    let report = discover_vst3_bundles([tree.path().to_path_buf()]).unwrap();
    assert_eq!(report.candidates().len(), 1);
    assert_eq!(report.failures().len(), 1);
    assert!(report.candidates()[0].source().ends_with("z/fixture.vst3"));

    let mut supervisor = AudioPluginSupervisor::scan(report, &mut FixtureLauncher, 2).unwrap();
    assert_eq!(supervisor.plugin_count(), 1);
    assert_eq!(
        supervisor.status(&identity()).unwrap().lifecycle(),
        AudioPluginLifecycle::Disabled
    );

    let checkpoint = state_with_version("2.3.0", 9);
    let activation_error = supervisor
        .enable(&identity(), Some(checkpoint.clone()))
        .unwrap_err();
    assert_eq!(activation_error.category(), ErrorCategory::Timeout);
    assert_eq!(
        supervisor.status(&identity()).unwrap().lifecycle(),
        AudioPluginLifecycle::Faulted
    );
    let registrations = registrations_from_native_audio(&supervisor).unwrap();
    assert_eq!(registrations.len(), 1);
    let registration = &registrations[0];
    assert_eq!(registration.lifecycle(), ExtensionLifecycle::Faulted);
    match registration.identity() {
        ExtensionRuntimeIdentity::NativeAudio {
            provider,
            format,
            vendor,
            identifier,
            version,
        } => {
            assert_eq!(provider.to_string(), "superi.audio-plugin-host@1.0.0");
            assert_eq!(*format, AudioPluginFormat::Vst3);
            assert_eq!(vendor, "Example Vendor");
            assert_eq!(identifier, "6E33225254224A00AA69301AF318797D");
            assert_eq!(version, "2.4.1");
        }
        ExtensionRuntimeIdentity::Versioned { .. } => panic!("expected native audio identity"),
    }
    assert!(registration.failure().is_some());
    assert!(!format!("{registration:?}").contains(tree.path().to_string_lossy().as_ref()));
    assert_eq!(
        supervisor
            .enable(&identity(), Some(checkpoint.clone()))
            .unwrap_err()
            .category(),
        ErrorCategory::Conflict
    );

    supervisor.recover(&identity()).unwrap();
    assert_eq!(
        supervisor.status(&identity()).unwrap().lifecycle(),
        AudioPluginLifecycle::Ready
    );
    assert_eq!(
        supervisor.recover(&identity()).unwrap_err().category(),
        ErrorCategory::Conflict
    );
    let captured = supervisor.capture_checkpoint(&identity()).unwrap();
    assert!(captured.identity().is_same_component(checkpoint.identity()));
    assert_eq!(captured.identity().version(), "2.4.1");
    assert_eq!(captured.component_state(), checkpoint.component_state());
    assert_eq!(captured.controller_state(), checkpoint.controller_state());
    let (processor, _readings) = supervisor
        .prepare_processor(&identity(), 48_000, ChannelLayout::stereo(), 256)
        .unwrap();
    assert_eq!(
        superi_audio::graph::AudioProcessor::latency_samples(&processor),
        9
    );

    let first_instance = AudioPluginInstanceId::from_raw(0xc014_1001);
    let second_instance = AudioPluginInstanceId::from_raw(0xc014_1002);
    let command = supervisor
        .checkpoint_command(first_instance, &identity())
        .unwrap();
    let mut document = document();
    let outcome = document.execute_extension_command(0, command).unwrap();
    let first_key = audio_plugin_project_key(first_instance).unwrap();
    let record = outcome
        .snapshot()
        .extension_records()
        .get(&first_key)
        .unwrap();
    let (decoded_instance, decoded_state) = audio_plugin_state_from_project_record(record).unwrap();
    assert_eq!(decoded_instance, first_instance);
    assert_eq!(decoded_state, captured);

    let second_state = state(4);
    let second_record = audio_plugin_project_record(
        second_instance,
        &second_state,
        AudioPluginLifecycle::Disabled,
    )
    .unwrap();
    assert_eq!(
        second_record.lifecycle(),
        ProjectExtensionLifecycle::Disabled
    );
    let outcome = document
        .execute_extension_command(
            outcome.snapshot().revision(),
            superi_project::extensions::ProjectExtensionCommand::upsert(second_record),
        )
        .unwrap();
    assert_eq!(outcome.snapshot().extension_records().len(), 2);

    let project_path = std::env::temp_dir().join(format!(
        "superi-audio-plugin-project-{}-{}.superi",
        std::process::id(),
        NEXT_TEMP.fetch_add(1, Ordering::Relaxed)
    ));
    {
        let mut database = ProjectDatabase::create(&project_path).unwrap();
        database.replace(&document.snapshot()).unwrap();
    }
    let reopened = ProjectDatabase::open_read_only(&project_path).unwrap();
    let reopened = reopened.load().unwrap();
    let reopened_snapshot = reopened.snapshot();
    let reopened_record = reopened_snapshot
        .extension_records()
        .get(&first_key)
        .unwrap();
    let (reopened_instance, reopened_state) =
        audio_plugin_state_from_project_record(reopened_record).unwrap();
    assert_eq!(reopened_instance, first_instance);
    assert_eq!(reopened_state, captured);
    let second_key = audio_plugin_project_key(second_instance).unwrap();
    let (reopened_second_instance, reopened_second_state) = audio_plugin_state_from_project_record(
        reopened_snapshot
            .extension_records()
            .get(&second_key)
            .unwrap(),
    )
    .unwrap();
    assert_eq!(reopened_second_instance, second_instance);
    assert_eq!(reopened_second_state, second_state);
    drop(reopened);
    let _ = fs::remove_file(&project_path);
}

#[test]
fn in_process_worker_contracts_are_rejected_before_native_scan() {
    assert_eq!(
        AudioPluginWorkerContract::new(
            AudioPluginWorkerIsolation::InProcess,
            1,
            1024,
            100,
            true,
            0,
        )
        .unwrap_err()
        .category(),
        ErrorCategory::PermissionDenied
    );
    assert_eq!(
        AudioPluginWorkerContract::new(
            AudioPluginWorkerIsolation::WorkerProcess,
            1,
            usize::MAX,
            u64::MAX,
            true,
            0,
        )
        .unwrap_err()
        .category(),
        ErrorCategory::InvalidInput
    );

    let tree = TempTree::new("wrong-domain");
    assert_eq!(
        discover_vst3_bundles([tree.path().to_path_buf()])
            .unwrap_err()
            .category(),
        ErrorCategory::Conflict
    );
}

#[test]
fn repeated_worker_failures_quarantine_only_the_affected_plugin() {
    let _background = ExecutionDomain::BackgroundJob.enter_current().unwrap();
    let tree = TempTree::new("quarantine");
    tree.bundle("fixture.vst3");
    let report = discover_vst3_bundles([tree.path().to_path_buf()]).unwrap();
    let mut supervisor = AudioPluginSupervisor::scan(report, &mut AlwaysFailLauncher, 2).unwrap();

    assert_eq!(
        supervisor
            .enable(&identity(), Some(state(8)))
            .unwrap_err()
            .category(),
        ErrorCategory::Timeout
    );
    assert_eq!(
        supervisor.status(&identity()).unwrap().lifecycle(),
        AudioPluginLifecycle::Faulted
    );
    assert_eq!(
        supervisor.recover(&identity()).unwrap_err().category(),
        ErrorCategory::Timeout
    );
    let status = supervisor.status(&identity()).unwrap();
    assert_eq!(status.lifecycle(), AudioPluginLifecycle::Quarantined);
    assert_eq!(status.total_failures(), 2);
    assert_eq!(status.consecutive_failures(), 2);
    assert_eq!(
        supervisor.recover(&identity()).unwrap_err().category(),
        ErrorCategory::PermissionDenied
    );
    assert_eq!(
        supervisor
            .enable(&identity(), Some(state(8)))
            .unwrap_err()
            .category(),
        ErrorCategory::PermissionDenied
    );
    let quarantined_record = audio_plugin_project_record(
        AudioPluginInstanceId::from_raw(0xc014_2001),
        &state(8),
        AudioPluginLifecycle::Quarantined,
    )
    .unwrap();
    assert_eq!(
        quarantined_record.lifecycle(),
        ProjectExtensionLifecycle::Enabled
    );
    assert!(quarantined_record.failure().is_none());
}
