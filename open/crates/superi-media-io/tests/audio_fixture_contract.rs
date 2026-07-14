use std::path::{Path, PathBuf};

use superi_core::ids::MediaId;
use superi_core::pixel::ChannelLayout;
use superi_core::time::Timebase;
use superi_media_io::demux::{MediaSource, MetadataValue, SourceLocation, SourceRequest};
use superi_media_io::operation::{MediaPriority, OperationContext};
use superi_media_io::pcm::{ByteOrder, PcmContainerKind, PcmContainerSource, PcmEncoding};
use superi_media_io::read::ReadOutcome;

fn fixture_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../test-fixtures/audio/synchronized-multichannel/v1")
}

fn operation() -> OperationContext {
    OperationContext::new(MediaPriority::Interactive)
}

fn expected_sample(frame: u32, sample_rate: u32, channel: usize) -> i16 {
    let onset = sample_rate / 100;
    let tail = sample_rate * 9 / 100;
    if frame < onset || frame >= tail {
        return 0;
    }

    let elapsed = frame - onset;
    let phase = i64::from((u64::from(elapsed) * 1_000 % u64::from(sample_rate)) as u32);
    let rate = i64::from(sample_rate);
    let four_phase = phase * 4;
    let triangle = if four_phase < rate {
        four_phase
    } else if four_phase < rate * 3 {
        rate * 2 - four_phase
    } else {
        four_phase - rate * 4
    };
    let gain = 768 * i64::try_from(channel + 1).expect("channel index must fit");
    i16::try_from(triangle * gain / rate).expect("fixture sample must fit PCM16")
}

#[test]
fn canonical_audio_fixture_preserves_timing_routing_sync_and_continuity() {
    let cases = [
        (
            441,
            "stereo-44100.wav",
            44_100,
            0x0003,
            ChannelLayout::stereo(),
        ),
        (
            480,
            "surround-5-1-48000.wav",
            48_000,
            0x003f,
            ChannelLayout::surround_5_1(),
        ),
        (
            960,
            "surround-7-1-96000.wav",
            96_000,
            0x063f,
            ChannelLayout::surround_7_1(),
        ),
    ];

    for (media_id, name, sample_rate, channel_mask, channel_layout) in cases {
        let channel_count = channel_layout.len();
        let frame_count = sample_rate / 10;
        let request = SourceRequest::new(
            MediaId::from_raw(media_id),
            SourceLocation::Path(fixture_root().join(name)),
        );
        let mut source = PcmContainerSource::open(&request, &operation())
            .unwrap_or_else(|error| panic!("{name} must open through the PCM source: {error}"));

        assert_eq!(source.container_kind(), PcmContainerKind::Wave);
        assert_eq!(source.format().encoding(), PcmEncoding::Integer);
        assert_eq!(source.format().byte_order(), ByteOrder::LittleEndian);
        assert_eq!(source.format().sample_rate(), sample_rate);
        assert_eq!(source.format().bits_per_sample(), 16);
        assert_eq!(source.format().valid_bits_per_sample(), 16);
        assert_eq!(source.format().block_align(), (channel_count * 2) as u16);
        assert_eq!(source.format().channel_layout(), &channel_layout);
        assert_eq!(source.frame_count(), u64::from(frame_count));
        assert_eq!(
            source
                .info()
                .duration()
                .expect("duration must exist")
                .timebase(),
            Timebase::integer(sample_rate).expect("sample rate must be valid")
        );
        assert_eq!(
            source
                .info()
                .duration()
                .expect("duration must exist")
                .value(),
            u64::from(frame_count)
        );
        assert_eq!(source.info().streams()[0].codec().as_str(), "pcm_s16le");
        assert_eq!(
            source.info().metadata().get("container.wav.channel_mask"),
            Some(&MetadataValue::Unsigned(channel_mask))
        );

        let packet = match source
            .read_packet(&operation())
            .expect("fixture read must succeed")
        {
            ReadOutcome::Complete(packet) => packet,
            ReadOutcome::Partial { .. } => panic!("canonical audio fixture must be complete"),
            ReadOutcome::EndOfStream => panic!("canonical audio fixture must contain samples"),
            _ => panic!("audio fixture returned an unknown read outcome"),
        };
        assert_eq!(
            packet
                .timing()
                .presentation_time()
                .expect("PTS must exist")
                .value(),
            0
        );
        assert_eq!(
            packet
                .timing()
                .duration()
                .expect("duration must exist")
                .value(),
            u64::from(frame_count)
        );
        assert!(matches!(
            source
                .read_packet(&operation())
                .expect("EOS read must succeed"),
            ReadOutcome::EndOfStream
        ));

        let samples = packet
            .data()
            .chunks_exact(2)
            .map(|sample| i16::from_le_bytes([sample[0], sample[1]]))
            .collect::<Vec<_>>();
        assert_eq!(samples.len(), frame_count as usize * channel_count);

        for frame in 0..frame_count {
            for channel in 0..channel_count {
                assert_eq!(
                    samples[frame as usize * channel_count + channel],
                    expected_sample(frame, sample_rate, channel),
                    "{name} frame {frame} channel {channel} must retain exact routing"
                );
            }
        }

        let onset = sample_rate / 100;
        let tail = sample_rate * 9 / 100;
        for channel in 0..channel_count {
            assert_eq!(samples[onset as usize * channel_count + channel], 0);
            assert_ne!(samples[(onset + 1) as usize * channel_count + channel], 0);
            assert_eq!(samples[tail as usize * channel_count + channel], 0);
        }
        assert!(samples[..onset as usize * channel_count]
            .iter()
            .all(|sample| *sample == 0));
        assert!(samples[tail as usize * channel_count..]
            .iter()
            .all(|sample| *sample == 0));

        let routed =
            &samples[(onset + 1) as usize * channel_count..(onset + 2) as usize * channel_count];
        assert!(
            routed.windows(2).all(|pair| pair[0].abs() < pair[1].abs()),
            "per-channel gains must make routing swaps observable"
        );

        for channel in 0..channel_count {
            let maximum_delta = (1..frame_count as usize)
                .map(|frame| {
                    let current = samples[frame * channel_count + channel];
                    let previous = samples[(frame - 1) * channel_count + channel];
                    (i32::from(current) - i32::from(previous)).abs()
                })
                .max()
                .expect("fixture must contain adjacent samples");
            assert!(
                maximum_delta <= 600,
                "{name} channel {channel} has discontinuity {maximum_delta}"
            );
        }
    }
}
