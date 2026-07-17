use superi_audio::graph::{AudioProcessBlock, AudioProcessor};
use superi_audio::plugins::{
    AudioPluginBridgeStatus, AudioPluginFormat, AudioPluginIdentity, AudioPluginState,
    IsolatedAudioPluginProcessBridge, PreparedIsolatedAudioPlugin, MAX_AUDIO_PLUGIN_STATE_BYTES,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{ErrorCategory, Result};
use superi_core::pixel::ChannelLayout;
use superi_core::time::SampleTime;

const RATE: u32 = 48_000;

#[test]
fn format_neutral_plugin_state_round_trips_exact_opaque_bytes_and_rejects_bad_envelopes() {
    let identity = AudioPluginIdentity::new(
        AudioPluginFormat::Vst3,
        "Example Vendor",
        "6E33225254224A00AA69301AF318797D",
        "2.4.1",
    )
    .unwrap();
    let state = AudioPluginState::new(
        identity.clone(),
        RATE,
        73,
        2,
        vec![0, 1, 0xfe, 0xff],
        vec![9, 8, 0, 7],
    )
    .unwrap();
    let encoded = state.encode().unwrap();
    assert_eq!(AudioPluginState::decode(&encoded).unwrap(), state);
    assert_eq!(state.identity(), &identity);
    assert_eq!(state.sample_rate(), RATE);
    assert_eq!(state.native_latency_samples(), 73);
    assert_eq!(state.transport_latency_samples(), 2);
    assert_eq!(state.total_latency_samples(), 75);

    let truncated = AudioPluginState::decode(&encoded[..encoded.len() - 1]).unwrap_err();
    assert_eq!(truncated.category(), ErrorCategory::CorruptData);

    let mut wrong_magic = encoded.clone();
    wrong_magic[0] ^= 0xff;
    assert_eq!(
        AudioPluginState::decode(&wrong_magic)
            .unwrap_err()
            .category(),
        ErrorCategory::CorruptData
    );

    let mut wrong_digest = encoded;
    let payload_byte = wrong_digest.len() - 33;
    wrong_digest[payload_byte] ^= 0x80;
    assert_eq!(
        AudioPluginState::decode(&wrong_digest)
            .unwrap_err()
            .category(),
        ErrorCategory::CorruptData
    );

    assert_eq!(
        AudioPluginState::new(
            identity,
            RATE,
            0,
            0,
            vec![0; MAX_AUDIO_PLUGIN_STATE_BYTES + 1],
            Vec::new(),
        )
        .unwrap_err()
        .category(),
        ErrorCategory::ResourceExhausted
    );
}

struct FaultingBridge {
    ring: [f32; 2],
    cursor: usize,
    calls: usize,
}

impl IsolatedAudioPluginProcessBridge for FaultingBridge {
    fn fixed_transport_latency_samples(&self) -> usize {
        2
    }

    fn try_process(
        &mut self,
        _start_time: SampleTime,
        input: &[f32],
        output: &mut [f32],
    ) -> Result<AudioPluginBridgeStatus> {
        self.calls += 1;
        if self.calls == 2 {
            return Ok(AudioPluginBridgeStatus::Faulted);
        }
        for (input, output) in input.iter().copied().zip(output.iter_mut()) {
            *output = self.ring[self.cursor] * 2.0;
            self.ring[self.cursor] = input;
            self.cursor = (self.cursor + 1) % self.ring.len();
        }
        Ok(AudioPluginBridgeStatus::Produced)
    }
}

#[test]
fn worker_faults_are_contained_with_a_timing_matched_dry_fallback() {
    let bridge = FaultingBridge {
        ring: [0.0; 2],
        cursor: 0,
        calls: 0,
    };
    let (mut plugin, readings) =
        PreparedIsolatedAudioPlugin::new(Box::new(bridge), RATE, ChannelLayout::mono(), 4, 0)
            .unwrap();
    assert_eq!(plugin.latency_samples(), 2);

    let layout = ChannelLayout::mono();
    let _audio = ExecutionDomain::Audio.enter_current().unwrap();
    let short_input = [9.0; 3];
    let mut short_output = [f32::NAN; 3];
    assert_eq!(
        plugin
            .process(AudioProcessBlock {
                start_time: SampleTime::new(0, RATE).unwrap(),
                frame_count: 4,
                input: Some(&short_input),
                input_layout: Some(&layout),
                output: &mut short_output,
                output_layout: &layout,
            })
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(readings.snapshot().processed_blocks(), 0);

    let first_input = [1.0, 2.0, 3.0, 4.0];
    let mut first_output = [0.0; 4];
    plugin
        .process(AudioProcessBlock {
            start_time: SampleTime::new(0, RATE).unwrap(),
            frame_count: 4,
            input: Some(&first_input),
            input_layout: Some(&layout),
            output: &mut first_output,
            output_layout: &layout,
        })
        .unwrap();
    assert_eq!(first_output, [0.0, 0.0, 2.0, 4.0]);

    let second_input = [5.0, 6.0, 7.0, 8.0];
    let mut second_output = [f32::NAN; 4];
    plugin
        .process(AudioProcessBlock {
            start_time: SampleTime::new(4, RATE).unwrap(),
            frame_count: 4,
            input: Some(&second_input),
            input_layout: Some(&layout),
            output: &mut second_output,
            output_layout: &layout,
        })
        .unwrap();
    assert_eq!(second_output, [3.0, 4.0, 5.0, 6.0]);

    let snapshot = readings.snapshot();
    assert!(snapshot.is_faulted());
    assert_eq!(snapshot.processed_blocks(), 2);
    assert_eq!(snapshot.produced_blocks(), 1);
    assert_eq!(snapshot.delayed_dry_blocks(), 1);
    assert_eq!(snapshot.worker_faults(), 1);
    assert_eq!(snapshot.last_start_sample(), Some(4));
}
