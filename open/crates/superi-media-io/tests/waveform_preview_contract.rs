use std::sync::Arc;

use superi_core::error::ErrorCategory;
use superi_core::pixel::{ChannelLayout, ChannelPosition, SampleFormat};
use superi_core::time::SampleTime;
use superi_image::preview::{WaveformPeak, WaveformRasterStyle};
use superi_media_io::audio_io::{AudioBlock, AudioFormat, AudioPlane};
use superi_media_io::preview::{generate_audio_waveform_image, WaveformRequest};

#[test]
fn decoded_blocks_generate_continuous_channel_ordered_waveform_peaks() {
    let format = AudioFormat::new(48_000, SampleFormat::I16, ChannelLayout::stereo()).unwrap();
    let first = packed_i16_block(
        format.clone(),
        -4,
        &[(-32_768, 0), (-16_384, 0), (0, -32_768), (16_384, -16_384)],
    );
    let second = packed_i16_block(
        format,
        0,
        &[(32_767, 0), (0, 16_384), (-8_192, 32_767), (8_192, 0)],
    );
    let original = [first.clone(), second.clone()];
    let request = WaveformRequest::new(
        4,
        WaveformRasterStyle::new(5, 1, [12, 34, 56, 255], [1, 2, 3, 0]).unwrap(),
    )
    .unwrap();

    let waveform =
        generate_audio_waveform_image(&[first.clone(), second.clone()], request).unwrap();

    assert_eq!(waveform.start(), SampleTime::new(-4, 48_000).unwrap());
    assert_eq!(waveform.frame_count(), 8);
    assert_eq!(waveform.channel_layout(), &ChannelLayout::stereo());
    assert_peak(waveform.peak(0, 0).unwrap(), -1.0, -0.5);
    assert_peak(waveform.peak(0, 1).unwrap(), 0.0, 0.0);
    assert_peak(waveform.peak(1, 0).unwrap(), 0.0, 0.5);
    assert_peak(waveform.peak(1, 1).unwrap(), -1.0, -0.5);
    assert_peak(waveform.peak(2, 0).unwrap(), 0.0, 32_767.0 / 32_768.0);
    assert_peak(waveform.peak(2, 1).unwrap(), 0.0, 0.5);
    assert_peak(waveform.peak(3, 0).unwrap(), -0.25, 0.25);
    assert_peak(waveform.peak(3, 1).unwrap(), 0.0, 32_767.0 / 32_768.0);
    assert_eq!(first, original[0]);
    assert_eq!(second, original[1]);
}

#[test]
fn every_decoded_sample_representation_has_deterministic_normalization() {
    let cases = [
        (SampleFormat::U8, vec![255], 127.0 / 128.0),
        (SampleFormat::U8Planar, vec![0], -1.0),
        (SampleFormat::I16, 16_384_i16.to_le_bytes().to_vec(), 0.5),
        (
            SampleFormat::I16Planar,
            (-16_384_i16).to_le_bytes().to_vec(),
            -0.5,
        ),
        (SampleFormat::I24, vec![0, 0, 64], 0.5),
        (SampleFormat::I24Planar, vec![0, 0, 192], -0.5),
        (
            SampleFormat::I32,
            1_073_741_824_i32.to_le_bytes().to_vec(),
            0.5,
        ),
        (
            SampleFormat::I32Planar,
            (-1_073_741_824_i32).to_le_bytes().to_vec(),
            -0.5,
        ),
        (SampleFormat::F32, 0.25_f32.to_le_bytes().to_vec(), 0.25),
        (
            SampleFormat::F32Planar,
            (-0.25_f32).to_le_bytes().to_vec(),
            -0.25,
        ),
        (SampleFormat::F64, f64::MAX.to_le_bytes().to_vec(), 1.0),
        (
            SampleFormat::F64Planar,
            (-0.75_f64).to_le_bytes().to_vec(),
            -0.75,
        ),
    ];

    for (sample_format, bytes, expected) in cases {
        let block = mono_block(sample_format, 17, bytes);
        let waveform = generate_audio_waveform_image(
            &[block],
            WaveformRequest::new(1, WaveformRasterStyle::default()).unwrap(),
        )
        .unwrap();
        assert_peak(waveform.peak(0, 0).unwrap(), expected, expected);
        assert_eq!(waveform.start(), SampleTime::new(17, 48_000).unwrap());
    }
}

#[test]
fn planar_channels_stay_separate_and_requested_width_is_capped_to_source_frames() {
    let layout =
        ChannelLayout::new([ChannelPosition::FrontRight, ChannelPosition::FrontLeft]).unwrap();
    let format = AudioFormat::new(48_000, SampleFormat::I16Planar, layout.clone()).unwrap();
    let right = [-32_768_i16, 0, 32_767]
        .into_iter()
        .flat_map(i16::to_le_bytes)
        .collect::<Vec<_>>();
    let left = [-8_192_i16, 8_192, 0]
        .into_iter()
        .flat_map(i16::to_le_bytes)
        .collect::<Vec<_>>();
    let block = AudioBlock::new(
        format,
        SampleTime::new(9, 48_000).unwrap(),
        3,
        vec![AudioPlane::new(right.into()), AudioPlane::new(left.into())],
    )
    .unwrap();

    let waveform = generate_audio_waveform_image(
        &[block],
        WaveformRequest::new(99, WaveformRasterStyle::default()).unwrap(),
    )
    .unwrap();

    assert_eq!(waveform.image().descriptor().data_window().width(), 3);
    assert_eq!(waveform.channel_layout(), &layout);
    assert_peak(waveform.peak(0, 0).unwrap(), -1.0, -1.0);
    assert_peak(waveform.peak(0, 1).unwrap(), -0.25, -0.25);
    assert_peak(
        waveform.peak(2, 0).unwrap(),
        32_767.0 / 32_768.0,
        32_767.0 / 32_768.0,
    );
    assert_peak(waveform.peak(2, 1).unwrap(), 0.0, 0.0);
}

#[test]
fn waveform_generation_rejects_gaps_overlaps_format_changes_and_nonfinite_audio() {
    let mono_i16 = AudioFormat::new(48_000, SampleFormat::I16, ChannelLayout::mono()).unwrap();
    let first = AudioBlock::new(
        mono_i16.clone(),
        SampleTime::new(0, 48_000).unwrap(),
        1,
        vec![AudioPlane::new(0_i16.to_le_bytes().to_vec().into())],
    )
    .unwrap();
    let gap = AudioBlock::new(
        mono_i16.clone(),
        SampleTime::new(2, 48_000).unwrap(),
        1,
        vec![AudioPlane::new(0_i16.to_le_bytes().to_vec().into())],
    )
    .unwrap();
    let overlap = AudioBlock::new(
        mono_i16,
        SampleTime::new(0, 48_000).unwrap(),
        1,
        vec![AudioPlane::new(0_i16.to_le_bytes().to_vec().into())],
    )
    .unwrap();
    let different = mono_block(SampleFormat::F32, 1, 0.0_f32.to_le_bytes().to_vec());
    let request = || WaveformRequest::new(1, WaveformRasterStyle::default()).unwrap();

    assert_eq!(
        generate_audio_waveform_image(&[first.clone(), gap], request())
            .unwrap_err()
            .category(),
        ErrorCategory::Conflict
    );
    assert_eq!(
        generate_audio_waveform_image(&[first.clone(), overlap], request())
            .unwrap_err()
            .category(),
        ErrorCategory::Conflict
    );
    assert_eq!(
        generate_audio_waveform_image(&[first, different], request())
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );

    for value in [f32::NAN, f32::INFINITY] {
        let block = mono_block(SampleFormat::F32, 0, value.to_le_bytes().to_vec());
        assert_eq!(
            generate_audio_waveform_image(&[block], request())
                .unwrap_err()
                .category(),
            ErrorCategory::CorruptData
        );
    }
    assert_eq!(
        generate_audio_waveform_image(&[], request())
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        WaveformRequest::new(0, WaveformRasterStyle::default())
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
}

#[test]
fn waveform_requests_are_safe_for_background_owners() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<WaveformRequest>();
}

fn packed_i16_block(format: AudioFormat, timestamp: i64, frames: &[(i16, i16)]) -> AudioBlock {
    let bytes = frames
        .iter()
        .flat_map(|(left, right)| left.to_le_bytes().into_iter().chain(right.to_le_bytes()))
        .collect::<Vec<_>>();
    AudioBlock::new(
        format,
        SampleTime::new(timestamp, 48_000).unwrap(),
        frames.len() as u64,
        vec![AudioPlane::new(bytes.into())],
    )
    .unwrap()
}

fn mono_block(sample_format: SampleFormat, timestamp: i64, bytes: Vec<u8>) -> AudioBlock {
    let format = AudioFormat::new(48_000, sample_format, ChannelLayout::mono()).unwrap();
    AudioBlock::new(
        format,
        SampleTime::new(timestamp, 48_000).unwrap(),
        1,
        vec![AudioPlane::new(Arc::from(bytes))],
    )
    .unwrap()
}

fn assert_peak(actual: WaveformPeak, minimum: f32, maximum: f32) {
    assert!((actual.minimum() - minimum).abs() < 0.000_001);
    assert!((actual.maximum() - maximum).abs() < 0.000_001);
}
