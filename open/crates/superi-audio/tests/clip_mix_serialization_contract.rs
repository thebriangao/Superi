use superi_audio::mixing::{ChannelMap, ClipMixControls, ClipMixMutation, ClipMixState};
use superi_audio::serialize::{deserialize_clip_mix_state, serialize_clip_mix_state};
use superi_core::error::ErrorCategory;
use superi_core::ids::ClipId;
use superi_core::pixel::{ChannelLayout, ChannelPosition};

fn route(source: ChannelPosition, destination: ChannelPosition, gain: f32) -> ChannelMap {
    ChannelMap::new(source, destination, gain).unwrap()
}

fn state() -> ClipMixState {
    let stereo = ChannelLayout::stereo();
    let surround = ChannelLayout::new([
        ChannelPosition::FrontLeft,
        ChannelPosition::FrontRight,
        ChannelPosition::FrontCenter,
        ChannelPosition::LowFrequency,
        ChannelPosition::SideLeft,
        ChannelPosition::SideRight,
    ])
    .unwrap();
    let controls = ClipMixControls::new(
        surround,
        stereo,
        [
            route(
                ChannelPosition::FrontLeft,
                ChannelPosition::FrontRight,
                0.25,
            ),
            route(ChannelPosition::FrontRight, ChannelPosition::FrontLeft, 0.5),
            route(
                ChannelPosition::FrontCenter,
                ChannelPosition::FrontLeft,
                0.75,
            ),
            route(
                ChannelPosition::SideRight,
                ChannelPosition::FrontRight,
                1.25,
            ),
        ],
    )
    .unwrap()
    .with_gain(f32::from_bits(0x3f40_0001))
    .unwrap()
    .with_fades(48_001, 96_001)
    .unwrap()
    .with_pan(f32::from_bits(0xbe80_0001))
    .unwrap()
    .with_muted(true)
    .with_solo(true)
    .with_phase_inverted([ChannelPosition::FrontRight])
    .unwrap();
    let mut state = ClipMixState::new();
    state
        .apply(
            0,
            &[ClipMixMutation::set(ClipId::from_raw(0xa11d), controls)],
        )
        .unwrap();
    state
}

#[test]
fn clip_mix_state_round_trips_canonically_with_exact_float_bits_and_route_order() {
    let state = state();
    let first = serialize_clip_mix_state(&state).unwrap();
    let decoded = deserialize_clip_mix_state(&first).unwrap();
    let second = serialize_clip_mix_state(&decoded).unwrap();

    assert_eq!(decoded, state);
    assert_eq!(second, first);
    let controls = decoded.controls(ClipId::from_raw(0xa11d)).unwrap();
    assert_eq!(controls.gain().to_bits(), 0x3f40_0001);
    assert_eq!(controls.pan().to_bits(), 0xbe80_0001);
    assert_eq!(controls.channel_map()[0].gain().to_bits(), 0x3e80_0000);
    assert_eq!(
        controls.channel_map()[0].source(),
        ChannelPosition::FrontLeft
    );
    assert_eq!(
        controls.channel_map()[3].destination(),
        ChannelPosition::FrontRight
    );
}

#[test]
fn clip_mix_decoder_rejects_tampering_unknown_fields_and_noncanonical_bytes() {
    let canonical = serialize_clip_mix_state(&state()).unwrap();

    let mut tampered = canonical.clone();
    let digit = tampered
        .iter()
        .position(|byte| *byte == b'4')
        .expect("fixture contains a digit");
    tampered[digit] = b'5';
    assert_eq!(
        deserialize_clip_mix_state(&tampered)
            .unwrap_err()
            .category(),
        ErrorCategory::CorruptData
    );

    let mut value: serde_json::Value = serde_json::from_slice(&canonical).unwrap();
    value
        .as_object_mut()
        .unwrap()
        .insert("unknown".to_owned(), serde_json::Value::Bool(true));
    let unknown = serde_json::to_vec(&value).unwrap();
    assert_eq!(
        deserialize_clip_mix_state(&unknown).unwrap_err().category(),
        ErrorCategory::CorruptData
    );

    let mut noncanonical = canonical;
    noncanonical.push(b' ');
    assert_eq!(
        deserialize_clip_mix_state(&noncanonical)
            .unwrap_err()
            .category(),
        ErrorCategory::CorruptData
    );
}
