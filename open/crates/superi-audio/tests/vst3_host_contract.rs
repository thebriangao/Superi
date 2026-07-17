use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};

use superi_audio::graph::{
    AudioBusKind, AudioEdge, AudioEdgeId, AudioGraph, AudioGraphId, AudioNode, AudioNodeId,
    AudioProcessBlock, AudioProcessor,
};
use superi_audio::hosting::vst3::{
    Vst3AutomationPoint, Vst3ClassId, Vst3EffectConfig, Vst3PluginState, Vst3ProcessMode,
    Vst3WorkerSession, VST3_SPEAKER_5_1, VST3_SPEAKER_7_1, VST3_SPEAKER_MONO, VST3_SPEAKER_QUAD,
    VST3_SPEAKER_STEREO,
};
use superi_audio::routing::SummingBus;
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{ErrorCategory, Result};
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::time::SampleTime;

const CLASS_TEXT: &str = "6E33225254224A00AA69301AF318797D";
const RATE: u32 = 48_000;
const MAXIMUM_FRAMES: usize = 8;

static TEMPORARY_ID: AtomicU64 = AtomicU64::new(0);

fn class_id() -> Vst3ClassId {
    Vst3ClassId::from_str(CLASS_TEXT).expect("canonical class id")
}

fn config(layout: ChannelLayout) -> Vst3EffectConfig {
    Vst3EffectConfig::new(
        PathBuf::from("fixture.vst3"),
        class_id(),
        48_000,
        layout,
        512,
    )
    .expect("valid VST3 configuration")
}

struct TemporaryBundle {
    root: PathBuf,
    bundle: PathBuf,
}

impl Drop for TemporaryBundle {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}

fn vst3_rlib() -> PathBuf {
    let executable = std::env::current_exe().expect("current integration-test executable");
    let dependencies = executable
        .parent()
        .expect("test executable dependency directory");
    let mut candidates = fs::read_dir(dependencies)
        .expect("read dependency directory")
        .filter_map(|entry| entry.ok().map(|entry| entry.path()))
        .filter(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.starts_with("libvst3-") && name.ends_with(".rlib"))
        })
        .collect::<Vec<_>>();
    candidates.sort();
    candidates
        .pop()
        .expect("the integration test links one compiled vst3 rlib")
}

fn build_fixture_bundle() -> TemporaryBundle {
    let id = TEMPORARY_ID.fetch_add(1, Ordering::Relaxed);
    let root =
        std::env::temp_dir().join(format!("superi-vst3-fixture-{}-{id}", std::process::id()));
    let bundle = root.join("SuperiVst3Fixture.vst3");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/vst3_gain.rs");
    let dependencies = std::env::current_exe()
        .expect("current integration-test executable")
        .parent()
        .expect("test executable dependency directory")
        .to_owned();

    #[cfg(target_os = "macos")]
    let binary = bundle.join("Contents/MacOS/SuperiVst3Fixture");
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    let binary = bundle.join("Contents/x86_64-linux/SuperiVst3Fixture.so");
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    let binary = bundle.join("Contents/aarch64-linux/SuperiVst3Fixture.so");
    #[cfg(all(target_os = "windows", target_arch = "x86_64"))]
    let binary = bundle.join("Contents/x86_64-win/SuperiVst3Fixture.vst3");
    #[cfg(all(target_os = "windows", target_arch = "aarch64"))]
    let binary = bundle.join("Contents/arm64-win/SuperiVst3Fixture.vst3");

    fs::create_dir_all(binary.parent().expect("fixture binary parent"))
        .expect("create VST3 fixture bundle");
    #[cfg(target_os = "macos")]
    {
        let resources = bundle.join("Contents/Resources");
        fs::create_dir_all(&resources).expect("create VST3 resource directory");
        fs::write(
            bundle.join("Contents/Info.plist"),
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
<key>CFBundleDevelopmentRegion</key><string>en</string>
<key>CFBundleExecutable</key><string>SuperiVst3Fixture</string>
<key>CFBundleIdentifier</key><string>dev.superi.vst3-fixture</string>
<key>CFBundleInfoDictionaryVersion</key><string>6.0</string>
<key>CFBundleName</key><string>SuperiVst3Fixture</string>
<key>CFBundlePackageType</key><string>BNDL</string>
<key>CFBundleVersion</key><string>1</string>
</dict></plist>
"#,
        )
        .expect("write VST3 fixture Info.plist");
    }

    let output = Command::new(std::env::var_os("RUSTC").unwrap_or_else(|| "rustc".into()))
        .arg("--edition=2021")
        .arg("--crate-name")
        .arg("superi_vst3_fixture")
        .arg("--crate-type=cdylib")
        .arg("-C")
        .arg("panic=abort")
        .arg("--extern")
        .arg(format!("vst3={}", vst3_rlib().display()))
        .arg("-L")
        .arg(format!("dependency={}", dependencies.display()))
        .arg(&source)
        .arg("-o")
        .arg(&binary)
        .output()
        .expect("compile VST3 fixture module");
    assert!(
        output.status.success(),
        "fixture compilation failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    TemporaryBundle { root, bundle }
}

fn node(raw: u128, input: bool, kind: Option<AudioBusKind>, layout: &ChannelLayout) -> AudioNode {
    if let Some(kind) = kind {
        AudioNode::bus(AudioNodeId::from_raw(raw), kind, layout.clone())
    } else {
        AudioNode::new(
            AudioNodeId::from_raw(raw),
            input.then(|| layout.clone()),
            layout.clone(),
        )
    }
}

fn edge(raw: u128, source: u128, destination: u128) -> AudioEdge {
    AudioEdge::new(
        AudioEdgeId::from_raw(raw),
        AudioNodeId::from_raw(source),
        AudioNodeId::from_raw(destination),
    )
}

struct ChannelSource;

impl AudioProcessor for ChannelSource {
    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
        let channels = block.output_layout.len();
        for frame in block.output.chunks_exact_mut(channels) {
            for (channel, sample) in frame.iter_mut().enumerate() {
                *sample = channel as f32 + 1.0;
            }
        }
        Ok(())
    }
}

fn selected_layout(name: &str) -> ChannelLayout {
    match name {
        "mono" => ChannelLayout::mono(),
        "stereo" => ChannelLayout::stereo(),
        "quad" => ChannelLayout::quad(),
        "5.1" => ChannelLayout::surround_5_1(),
        "7.1" => ChannelLayout::surround_7_1(),
        _ => panic!("unsupported child layout {name}"),
    }
}

fn packed_sequence(events: &[u64]) -> String {
    let mut packed = 0_u64;
    for (index, event) in events.iter().copied().enumerate() {
        packed |= event << (index * 4);
    }
    format!("{packed:X}")
}

fn expected_sequence() -> String {
    packed_sequence(&[1, 2, 3, 4, 4, 5, 6, 7, 7, 8, 9, 10, 10, 11, 12])
}

fn fixture_fields(evidence: &str) -> BTreeMap<&str, &str> {
    evidence
        .lines()
        .filter_map(|line| line.split_once('='))
        .collect()
}

fn assert_fixture_evidence(evidence: &str, channels: usize, mode: i32) {
    let fields = fixture_fields(evidence);
    assert_eq!(fields.get("events"), Some(&"15"));
    assert_eq!(fields.get("sequence"), Some(&expected_sequence().as_str()));
    assert_eq!(fields.get("sample_rate"), Some(&"48000"));
    assert_eq!(fields.get("start_sample"), Some(&"8"));
    assert_eq!(fields.get("frames"), Some(&"4"));
    assert_eq!(fields.get("mode"), Some(&mode.to_string().as_str()));
    assert_eq!(fields.get("sample_size"), Some(&"0"));
    assert_eq!(fields.get("channels"), Some(&channels.to_string().as_str()));
    assert_eq!(fields.get("processes"), Some(&"2"));
    assert_eq!(fields.get("host_objects"), Some(&"1"));
    assert_eq!(fields.get("callback_allocations"), Some(&"0"));
    assert_eq!(fields.get("component_state_gets"), Some(&"1"));
    assert_eq!(fields.get("component_state_sets"), Some(&"1"));
    assert_eq!(fields.get("controller_component_state_sets"), Some(&"1"));
    assert_eq!(fields.get("controller_state_gets"), Some(&"1"));
    assert_eq!(fields.get("controller_state_sets"), Some(&"1"));
}

#[test]
fn class_identity_is_strict_canonical_and_round_trips_four_words() {
    let id = class_id();
    assert_eq!(
        id.words(),
        [0x6E33_2252, 0x5422_4A00, 0xAA69_301A, 0xF318_797D]
    );
    assert_eq!(id.to_string(), CLASS_TEXT);
    assert_eq!(
        Vst3ClassId::from_str(&CLASS_TEXT.to_ascii_lowercase()).unwrap(),
        id
    );

    for malformed in [
        "",
        "6E332252-5422-4A00-AA69-301AF318797D",
        "6E33225254224A00AA69301AF318797",
        "6E33225254224A00AA69301AF318797D00",
        "6E33225254224A00AA69301AF318797G",
    ] {
        assert!(
            Vst3ClassId::from_str(malformed).is_err(),
            "accepted {malformed}"
        );
    }
}

#[test]
fn canonical_superi_layouts_map_to_exact_vst3_speaker_arrangements() {
    let cases = [
        (ChannelLayout::mono(), VST3_SPEAKER_MONO),
        (ChannelLayout::stereo(), VST3_SPEAKER_STEREO),
        (ChannelLayout::quad(), VST3_SPEAKER_QUAD),
        (ChannelLayout::surround_5_1(), VST3_SPEAKER_5_1),
        (ChannelLayout::surround_7_1(), VST3_SPEAKER_7_1),
    ];

    for (layout, expected) in cases {
        let config = config(layout.clone());
        assert_eq!(config.layout(), &layout);
        assert_eq!(config.speaker_arrangement(), expected);
    }

    let unsupported = ChannelLayout::new([
        ChannelPosition::FrontLeft,
        ChannelPosition::FrontRight,
        ChannelPosition::FrontCenter,
    ])
    .unwrap();
    assert_eq!(
        Vst3EffectConfig::new(
            PathBuf::from("fixture.vst3"),
            class_id(),
            48_000,
            unsupported,
            512,
        )
        .unwrap_err()
        .category(),
        ErrorCategory::Unsupported
    );
}

#[test]
fn process_mode_and_every_callback_bound_are_explicit_and_positive() {
    let realtime = config(ChannelLayout::stereo());
    assert_eq!(realtime.process_mode(), Vst3ProcessMode::Realtime);
    assert_eq!(realtime.automation_capacity(), 1_024);
    assert_eq!(realtime.maximum_automation_points_per_block(), 256);
    assert_eq!(realtime.monitoring_capacity(), 1_024);

    let offline = realtime
        .with_process_mode(Vst3ProcessMode::Offline)
        .with_automation_limits(32, 8)
        .unwrap()
        .with_monitoring_capacity(64)
        .unwrap();
    assert_eq!(offline.process_mode(), Vst3ProcessMode::Offline);
    assert_eq!(offline.automation_capacity(), 32);
    assert_eq!(offline.maximum_automation_points_per_block(), 8);
    assert_eq!(offline.monitoring_capacity(), 64);

    assert_eq!(
        config(ChannelLayout::stereo())
            .with_automation_limits(0, 1)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        config(ChannelLayout::stereo())
            .with_automation_limits(1, 0)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        config(ChannelLayout::stereo())
            .with_monitoring_capacity(0)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        config(ChannelLayout::stereo())
            .with_automation_limits(8, 9)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        config(ChannelLayout::stereo())
            .with_automation_limits(1_048_577, 1)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        config(ChannelLayout::stereo())
            .with_monitoring_capacity(1_048_577)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
}

#[test]
fn configuration_rejects_invalid_clock_path_and_block_bounds_before_loading() {
    for (path, sample_rate, frames) in [
        (PathBuf::new(), 48_000, 512),
        (PathBuf::from("fixture.vst3"), 0, 512),
        (PathBuf::from("fixture.vst3"), 48_000, 0),
        (PathBuf::from("fixture.vst3"), 48_000, 1_048_577),
    ] {
        assert_eq!(
            Vst3EffectConfig::new(
                path,
                class_id(),
                sample_rate,
                ChannelLayout::stereo(),
                frames,
            )
            .unwrap_err()
            .category(),
            ErrorCategory::InvalidInput
        );
    }
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
#[test]
fn explicit_loader_rejects_a_missing_module_without_partial_success() {
    let missing = std::env::temp_dir().join(format!(
        "superi-vst3-missing-{}-{}",
        std::process::id(),
        TEMPORARY_ID.fetch_add(1, Ordering::Relaxed)
    ));
    let request = Vst3EffectConfig::new(
        missing,
        class_id(),
        RATE,
        ChannelLayout::stereo(),
        MAXIMUM_FRAMES,
    )
    .unwrap();
    let error = match Vst3WorkerSession::load(request) {
        Ok(_) => panic!("missing VST3 module unexpectedly loaded"),
        Err(error) => error,
    };
    assert_eq!(error.category(), ErrorCategory::Unavailable);
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
#[test]
fn real_fixture_is_child_loaded_and_preserves_every_supported_layout_through_master() {
    let fixture = build_fixture_bundle();
    let executable = std::env::current_exe().expect("current integration-test executable");
    let cases = [
        ("mono", VST3_SPEAKER_MONO, Vst3ProcessMode::Realtime, false),
        (
            "stereo",
            VST3_SPEAKER_STEREO,
            Vst3ProcessMode::Offline,
            false,
        ),
        ("quad", VST3_SPEAKER_QUAD, Vst3ProcessMode::Realtime, true),
        ("5.1", VST3_SPEAKER_5_1, Vst3ProcessMode::Realtime, false),
        ("7.1", VST3_SPEAKER_7_1, Vst3ProcessMode::Realtime, false),
    ];

    for (name, arrangement, mode, overflow) in cases {
        let evidence = fixture.root.join(format!("{name}.evidence"));
        assert!(
            !evidence.exists(),
            "parent process loaded the fixture early"
        );
        let output = Command::new(&executable)
            .arg("--exact")
            .arg("vst3_fixture_child")
            .arg("--ignored")
            .arg("--nocapture")
            .arg("--test-threads=1")
            .env("SUPERI_VST3_FIXTURE_CHILD", "1")
            .env("SUPERI_VST3_FIXTURE_BUNDLE", &fixture.bundle)
            .env("SUPERI_VST3_FIXTURE_EVIDENCE", &evidence)
            .env("SUPERI_VST3_FIXTURE_LAYOUT", name)
            .env("SUPERI_VST3_FIXTURE_ARRANGEMENT", arrangement.to_string())
            .env(
                "SUPERI_VST3_FIXTURE_MODE",
                if mode == Vst3ProcessMode::Offline {
                    "offline"
                } else {
                    "realtime"
                },
            )
            .env(
                "SUPERI_VST3_FIXTURE_OVERFLOW",
                if overflow { "1" } else { "0" },
            )
            .output()
            .expect("run isolated VST3 fixture child");
        assert!(
            output.status.success(),
            "{name} child failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        let evidence = fs::read_to_string(&evidence).expect("child wrote lifecycle evidence");
        assert_fixture_evidence(
            &evidence,
            selected_layout(name).len(),
            if mode == Vst3ProcessMode::Offline {
                2
            } else {
                0
            },
        );
    }

    let unsupported_evidence = fixture.root.join("unsupported-context.evidence");
    let output = Command::new(&executable)
        .arg("--exact")
        .arg("vst3_fixture_child")
        .arg("--ignored")
        .arg("--nocapture")
        .arg("--test-threads=1")
        .env("SUPERI_VST3_FIXTURE_CHILD", "1")
        .env("SUPERI_VST3_FIXTURE_BUNDLE", &fixture.bundle)
        .env("SUPERI_VST3_FIXTURE_EVIDENCE", &unsupported_evidence)
        .env("SUPERI_VST3_FIXTURE_LAYOUT", "stereo")
        .env(
            "SUPERI_VST3_FIXTURE_ARRANGEMENT",
            VST3_SPEAKER_STEREO.to_string(),
        )
        .env("SUPERI_VST3_FIXTURE_MODE", "realtime")
        .env("SUPERI_VST3_FIXTURE_OVERFLOW", "0")
        .env("SUPERI_VST3_FIXTURE_REQUIRE_TEMPO", "1")
        .output()
        .expect("run isolated unsupported-context fixture child");
    assert!(
        output.status.success(),
        "unsupported-context child failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let evidence = fs::read_to_string(&unsupported_evidence)
        .expect("unsupported-context child wrote unwind evidence");
    let fields = fixture_fields(&evidence);
    assert_eq!(fields.get("events"), Some(&"4"));
    assert_eq!(
        fields.get("sequence"),
        Some(&packed_sequence(&[1, 2, 11, 12]).as_str())
    );
    assert_eq!(fields.get("host_objects"), Some(&"1"));
    assert_eq!(fields.get("processes"), Some(&"0"));

    let partial_evidence = fixture.root.join("partial-activation.evidence");
    let output = Command::new(&executable)
        .arg("--exact")
        .arg("vst3_fixture_child")
        .arg("--ignored")
        .arg("--nocapture")
        .arg("--test-threads=1")
        .env("SUPERI_VST3_FIXTURE_CHILD", "1")
        .env("SUPERI_VST3_FIXTURE_BUNDLE", &fixture.bundle)
        .env("SUPERI_VST3_FIXTURE_EVIDENCE", &partial_evidence)
        .env("SUPERI_VST3_FIXTURE_LAYOUT", "stereo")
        .env(
            "SUPERI_VST3_FIXTURE_ARRANGEMENT",
            VST3_SPEAKER_STEREO.to_string(),
        )
        .env("SUPERI_VST3_FIXTURE_MODE", "realtime")
        .env("SUPERI_VST3_FIXTURE_OVERFLOW", "0")
        .env("SUPERI_VST3_FIXTURE_FAIL_OUTPUT_ACTIVATION", "1")
        .output()
        .expect("run isolated partial-activation fixture child");
    assert!(
        output.status.success(),
        "partial-activation child failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let evidence = fs::read_to_string(&partial_evidence)
        .expect("partial-activation child wrote unwind evidence");
    let fields = fixture_fields(&evidence);
    assert_eq!(fields.get("events"), Some(&"8"));
    assert_eq!(
        fields.get("sequence"),
        Some(&packed_sequence(&[1, 2, 3, 4, 13, 10, 11, 12]).as_str())
    );
    assert_eq!(fields.get("host_objects"), Some(&"1"));
    assert_eq!(fields.get("processes"), Some(&"0"));

    let retained_evidence = fixture.root.join("retained-on-stop-failure.evidence");
    let output = Command::new(&executable)
        .arg("--exact")
        .arg("vst3_fixture_child")
        .arg("--ignored")
        .arg("--nocapture")
        .arg("--test-threads=1")
        .env("SUPERI_VST3_FIXTURE_CHILD", "1")
        .env("SUPERI_VST3_FIXTURE_BUNDLE", &fixture.bundle)
        .env("SUPERI_VST3_FIXTURE_EVIDENCE", &retained_evidence)
        .env("SUPERI_VST3_FIXTURE_LAYOUT", "stereo")
        .env(
            "SUPERI_VST3_FIXTURE_ARRANGEMENT",
            VST3_SPEAKER_STEREO.to_string(),
        )
        .env("SUPERI_VST3_FIXTURE_MODE", "realtime")
        .env("SUPERI_VST3_FIXTURE_OVERFLOW", "0")
        .env("SUPERI_VST3_FIXTURE_FAIL_STOP", "1")
        .output()
        .expect("run isolated failed-stop fixture child");
    assert!(
        output.status.success(),
        "failed-stop child failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !retained_evidence.exists(),
        "failed shutdown called module exit instead of retaining code until worker exit"
    );
}

#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
#[test]
#[ignore = "executed in an isolated subprocess by the parent fixture test"]
fn vst3_fixture_child() {
    if std::env::var_os("SUPERI_VST3_FIXTURE_CHILD").is_none() {
        return;
    }
    let bundle = PathBuf::from(
        std::env::var_os("SUPERI_VST3_FIXTURE_BUNDLE").expect("child fixture bundle path"),
    );
    let layout_name = std::env::var("SUPERI_VST3_FIXTURE_LAYOUT").expect("child layout name");
    let layout = selected_layout(&layout_name);
    let mode = if std::env::var("SUPERI_VST3_FIXTURE_MODE").as_deref() == Ok("offline") {
        Vst3ProcessMode::Offline
    } else {
        Vst3ProcessMode::Realtime
    };
    let monitoring_overflow = std::env::var("SUPERI_VST3_FIXTURE_OVERFLOW").as_deref() == Ok("1");

    let mut request =
        Vst3EffectConfig::new(bundle, class_id(), RATE, layout.clone(), MAXIMUM_FRAMES)
            .unwrap()
            .with_process_mode(mode)
            .with_automation_limits(16, 8)
            .unwrap();
    request = request
        .with_monitoring_capacity(if monitoring_overflow { 1 } else { 8 })
        .unwrap();
    let initial_gain_state = 1.0_f64.to_bits().to_le_bytes().to_vec();
    request = request.with_initial_state(
        Vst3PluginState::new(initial_gain_state.clone(), initial_gain_state.clone()).unwrap(),
    );
    if std::env::var_os("SUPERI_VST3_FIXTURE_REQUIRE_TEMPO").is_some() {
        let error = match Vst3WorkerSession::load(request) {
            Ok(_) => panic!("VST3 effect with unsupported context requirements loaded"),
            Err(error) => error,
        };
        assert_eq!(error.category(), ErrorCategory::Unsupported);
        return;
    }
    if std::env::var_os("SUPERI_VST3_FIXTURE_FAIL_OUTPUT_ACTIVATION").is_some() {
        let error = match Vst3WorkerSession::load(request) {
            Ok(_) => panic!("VST3 effect with rejected output activation loaded"),
            Err(error) => error,
        };
        assert_eq!(error.category(), ErrorCategory::Unavailable);
        return;
    }
    let (mut session, mut effect, mut writer, mut readings) =
        Vst3WorkerSession::load(request).expect("load fixture VST3 effect");

    let captured = {
        let _background = ExecutionDomain::BackgroundJob.enter_current().unwrap();
        effect.capture_state().expect("capture fixture VST3 state")
    };
    assert_eq!(captured.component_state(), initial_gain_state);
    assert_eq!(captured.controller_state(), initial_gain_state);

    assert_eq!(session.metadata().factory_vendor(), "Superi");
    assert_eq!(
        session.metadata().component_name(),
        "Superi VST3 gain fixture"
    );
    assert_eq!(session.metadata().layout(), &layout);
    assert_eq!(session.metadata().process_mode(), mode);
    assert_eq!(session.metadata().latency_samples(), 7);
    assert_eq!(session.metadata().tail_samples(), 11);
    assert_eq!(session.metadata().parameters().len(), 2);
    assert_eq!(session.metadata().parameters()[0].id(), 0);
    assert!(session.metadata().parameters()[0].is_automatable());
    assert_eq!(session.metadata().parameters()[1].id(), 1);
    assert!(session.metadata().parameters()[1].is_read_only());
    assert_eq!(writer.parameter_ids(), [0]);

    let rejected = [
        Vst3AutomationPoint::new(0, SampleTime::new(1, RATE).unwrap(), 0.125).unwrap(),
        Vst3AutomationPoint::new(1, SampleTime::new(1, RATE).unwrap(), 0.125).unwrap(),
    ];
    assert_eq!(
        writer.submit(&rejected).unwrap_err().category(),
        ErrorCategory::NotFound
    );
    let points = [
        Vst3AutomationPoint::new(0, SampleTime::new(2, RATE).unwrap(), 0.5).unwrap(),
        Vst3AutomationPoint::new(0, SampleTime::new(5, RATE).unwrap(), 0.25).unwrap(),
        Vst3AutomationPoint::new(0, SampleTime::new(8, RATE).unwrap(), 0.75).unwrap(),
    ];
    writer.submit(&points).unwrap();

    let mut editable = AudioGraph::new(AudioGraphId::from_raw(12), RATE, MAXIMUM_FRAMES).unwrap();
    editable.insert_node(node(1, false, None, &layout)).unwrap();
    editable.insert_node(node(2, true, None, &layout)).unwrap();
    editable
        .insert_node(node(3, true, Some(AudioBusKind::Submix), &layout))
        .unwrap();
    editable
        .insert_node(node(4, true, Some(AudioBusKind::Master), &layout))
        .unwrap();
    editable.insert_edge(edge(1, 1, 2)).unwrap();
    editable.insert_edge(edge(2, 2, 3)).unwrap();
    editable.insert_edge(edge(3, 3, 4)).unwrap();
    let mut processors: BTreeMap<AudioNodeId, Box<dyn AudioProcessor>> = BTreeMap::new();
    processors.insert(AudioNodeId::from_raw(1), Box::new(ChannelSource));
    processors.insert(AudioNodeId::from_raw(2), Box::new(effect));
    processors.insert(AudioNodeId::from_raw(3), Box::new(SummingBus));
    processors.insert(AudioNodeId::from_raw(4), Box::new(SummingBus));
    let mut graph = editable.prepare_master(processors).unwrap();

    let channels = layout.len();
    let mut first = vec![0.0; MAXIMUM_FRAMES * channels];
    let mut second = vec![0.0; 4 * channels];
    {
        let _audio = ExecutionDomain::Audio.enter_current().unwrap();
        graph
            .process(
                SampleTime::new(0, RATE).unwrap(),
                MAXIMUM_FRAMES,
                &mut first,
            )
            .unwrap();
        graph
            .process(SampleTime::new(8, RATE).unwrap(), 4, &mut second)
            .unwrap();
    }

    for frame in 0..MAXIMUM_FRAMES {
        let gain = if frame < 2 {
            1.0
        } else if frame < 5 {
            0.5
        } else {
            0.25
        };
        for channel in 0..channels {
            assert_eq!(
                first[frame * channels + channel],
                (channel as f32 + 1.0) * gain
            );
        }
    }
    for frame in second.chunks_exact(channels) {
        for (channel, sample) in frame.iter().enumerate() {
            assert_eq!(*sample, (channel as f32 + 1.0) * 0.75);
        }
    }
    assert_eq!(graph.next_sample(), Some(12));

    let monitored = readings.drain_output_points(8);
    if monitoring_overflow {
        assert_eq!(monitored.len(), 1);
        assert_eq!(readings.telemetry().monitoring_overflow(), 2);
    } else {
        assert_eq!(monitored.len(), 3);
        assert_eq!(
            monitored
                .iter()
                .map(|point| point.sample_time().sample())
                .collect::<Vec<_>>(),
            [2, 5, 8]
        );
        assert_eq!(
            monitored
                .iter()
                .map(|point| point.normalized_value())
                .collect::<Vec<_>>(),
            [0.5, 0.25, 0.75]
        );
        assert_eq!(readings.telemetry().monitoring_overflow(), 0);
    }
    assert_eq!(readings.telemetry().processed_blocks(), 2);
    assert_eq!(readings.telemetry().last_start_sample(), Some(8));
    assert_eq!(readings.telemetry().process_failures(), 0);
    assert_eq!(readings.telemetry().nonfinite_output_failures(), 0);

    assert_eq!(
        session.shutdown().unwrap_err().category(),
        ErrorCategory::Conflict
    );
    assert_eq!(readings.telemetry().shutdown_order_violations(), 1);
    drop(graph);
    let evidence_path = PathBuf::from(std::env::var_os("SUPERI_VST3_FIXTURE_EVIDENCE").unwrap());
    if std::env::var_os("SUPERI_VST3_FIXTURE_FAIL_STOP").is_some() {
        assert_eq!(
            session.shutdown().unwrap_err().category(),
            ErrorCategory::Unavailable
        );
        assert!(!session.is_shutdown());
        assert!(!evidence_path.exists());
        return;
    }
    session.shutdown().unwrap();
    assert!(session.is_shutdown());

    let evidence = fs::read_to_string(evidence_path).expect("module exit wrote evidence");
    assert_fixture_evidence(
        &evidence,
        channels,
        if mode == Vst3ProcessMode::Offline {
            2
        } else {
            0
        },
    );
}
