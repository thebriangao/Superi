use std::sync::Arc;
use std::thread;
use std::time::{Duration as StdDuration, Instant};

use superi_concurrency::clock::{
    AudioClockUpdate, AudioMasterClock, PlaybackClock, PlaybackClockMode,
};
use superi_core::error::{ErrorCategory, Recoverability, Result};
use superi_core::time::{RationalTime, SampleTime, Timebase};

fn instant_after(anchor: Instant, duration: StdDuration) -> Instant {
    anchor.checked_add(duration).unwrap()
}

#[test]
fn modes_have_stable_public_identity() {
    assert_eq!(
        PlaybackClockMode::ALL,
        &[PlaybackClockMode::Playback, PlaybackClockMode::AudioMaster,]
    );
    assert_eq!(PlaybackClockMode::Playback.code(), "playback");
    assert_eq!(PlaybackClockMode::AudioMaster.code(), "audio_master");
    for mode in PlaybackClockMode::ALL {
        assert_eq!(PlaybackClockMode::from_code(mode.code()), Some(*mode));
    }
    assert_eq!(PlaybackClockMode::from_code("wall_clock"), None);
    assert_eq!(AudioClockUpdate::Advanced.code(), "advanced");
    assert_eq!(AudioClockUpdate::Unchanged.code(), "unchanged");
}

#[test]
fn playback_mode_uses_a_checked_anchor_without_accumulating_rounding() -> Result<()> {
    let timeline_timebase = Timebase::integer(48_000)?;
    let timeline_anchor = RationalTime::new(96_000, timeline_timebase);
    let monotonic_anchor = Instant::now();
    let clock = PlaybackClock::playback(timeline_anchor, monotonic_anchor);

    assert_eq!(clock.mode(), PlaybackClockMode::Playback);
    assert_eq!(clock.timeline_timebase(), timeline_timebase);
    assert_eq!(clock.position_at(monotonic_anchor)?, timeline_anchor);
    assert_eq!(
        clock.position_at(instant_after(
            monotonic_anchor,
            StdDuration::from_millis(250)
        ))?,
        RationalTime::new(108_000, timeline_timebase)
    );

    let one_hour = instant_after(monotonic_anchor, StdDuration::from_secs(3_600));
    let direct = clock.position_at(one_hour)?;
    for second in 1..=3_600 {
        let sampled = clock.position_at(instant_after(
            monotonic_anchor,
            StdDuration::from_secs(second),
        ))?;
        if second == 3_600 {
            assert_eq!(sampled, direct);
        }
    }
    assert_eq!(direct, RationalTime::new(172_896_000, timeline_timebase));

    let reversed = clock
        .position_at(
            monotonic_anchor
                .checked_sub(StdDuration::from_nanos(1))
                .unwrap(),
        )
        .unwrap_err();
    assert_eq!(reversed.category(), ErrorCategory::Conflict);
    assert_eq!(reversed.recoverability(), Recoverability::Retryable);
    assert_eq!(
        reversed.contexts()[0].component(),
        "superi-concurrency.clock"
    );
    assert_eq!(reversed.contexts()[0].operation(), "read_position");
    assert_eq!(reversed.contexts()[0].field("mode"), Some("playback"));

    Ok(())
}

#[test]
fn audio_master_mode_tracks_the_audible_sample_clock_exactly() -> Result<()> {
    let sample_rate = 48_000;
    let source_anchor = SampleTime::new(-2_400, sample_rate)?;
    let source = Arc::new(AudioMasterClock::new(source_anchor));
    let timeline_anchor = RationalTime::new(240_000, source_anchor.timebase());
    let clock = PlaybackClock::audio_master(timeline_anchor, Arc::clone(&source))?;

    assert_eq!(clock.mode(), PlaybackClockMode::AudioMaster);
    assert_eq!(source.sample_rate(), sample_rate);
    assert_eq!(source.position(), source_anchor);
    assert_eq!(clock.position()?, timeline_anchor);

    assert_eq!(source.publish_sample(45_600)?, AudioClockUpdate::Advanced);
    assert_eq!(
        clock.position()?,
        RationalTime::new(288_000, source_anchor.timebase())
    );
    assert_eq!(
        source.publish(SampleTime::new(45_600, sample_rate)?)?,
        AudioClockUpdate::Unchanged
    );

    let one_hour_sample = 45_600 + i64::from(sample_rate) * 3_600;
    source.publish_sample(one_hour_sample)?;
    assert_eq!(
        clock.position()?,
        RationalTime::new(173_088_000, source_anchor.timebase())
    );

    Ok(())
}

#[test]
fn audio_publication_rejects_frequency_changes_and_position_regressions() -> Result<()> {
    let source = AudioMasterClock::new(SampleTime::new(1_000, 48_000)?);

    let regression = source.publish_sample(999).unwrap_err();
    assert_eq!(regression.category(), ErrorCategory::Conflict);
    assert_eq!(regression.recoverability(), Recoverability::Retryable);
    assert_eq!(
        regression.contexts()[0].operation(),
        "publish_audio_position"
    );
    assert_eq!(
        regression.contexts()[0].field("current_sample"),
        Some("1000")
    );
    assert_eq!(
        regression.contexts()[0].field("observed_sample"),
        Some("999")
    );
    assert_eq!(source.position().sample(), 1_000);

    let mismatch = source.publish(SampleTime::new(2_000, 44_100)?).unwrap_err();
    assert_eq!(mismatch.category(), ErrorCategory::InvalidInput);
    assert_eq!(mismatch.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(
        mismatch.contexts()[0].field("expected_sample_rate"),
        Some("48000")
    );
    assert_eq!(
        mismatch.contexts()[0].field("actual_sample_rate"),
        Some("44100")
    );
    assert_eq!(source.position().sample(), 1_000);

    let timeline_mismatch = PlaybackClock::audio_master(
        RationalTime::new(0, Timebase::NANOSECONDS),
        Arc::new(source),
    )
    .unwrap_err();
    assert_eq!(timeline_mismatch.category(), ErrorCategory::InvalidInput);
    assert_eq!(
        timeline_mismatch.recoverability(),
        Recoverability::UserCorrectable
    );
    assert_eq!(
        timeline_mismatch.contexts()[0].operation(),
        "anchor_audio_master"
    );

    Ok(())
}

#[test]
fn switching_modes_and_reanchoring_preserve_explicit_timeline_continuity() -> Result<()> {
    let timebase = Timebase::integer(48_000)?;
    let playback_anchor = Instant::now();
    let mut clock = PlaybackClock::playback(RationalTime::new(480_000, timebase), playback_anchor);
    let quarter_second = instant_after(playback_anchor, StdDuration::from_millis(250));
    let audio = Arc::new(AudioMasterClock::new(SampleTime::new(5_000, 48_000)?));

    assert_eq!(
        clock.switch_to_audio_master_at(Arc::clone(&audio), quarter_second)?,
        RationalTime::new(492_000, timebase)
    );
    assert_eq!(clock.mode(), PlaybackClockMode::AudioMaster);
    assert_eq!(clock.position()?, RationalTime::new(492_000, timebase));

    audio.publish_sample(9_800)?;
    assert_eq!(clock.position()?, RationalTime::new(496_800, timebase));

    let half_second = instant_after(playback_anchor, StdDuration::from_millis(500));
    assert_eq!(
        clock.switch_to_playback_at(half_second)?,
        RationalTime::new(496_800, timebase)
    );
    assert_eq!(clock.mode(), PlaybackClockMode::Playback);
    assert_eq!(
        clock.position_at(instant_after(half_second, StdDuration::from_millis(100)))?,
        RationalTime::new(501_600, timebase)
    );

    let reanchor_at = instant_after(half_second, StdDuration::from_millis(125));
    let seek_target = RationalTime::new(-48_000, timebase);
    clock.reanchor_at(seek_target, reanchor_at)?;
    assert_eq!(clock.position_at(reanchor_at)?, seek_target);
    assert_eq!(
        clock.position_at(instant_after(reanchor_at, StdDuration::from_secs(1)))?,
        RationalTime::new(0, timebase)
    );

    Ok(())
}

#[test]
fn clocks_cross_threads_without_transferring_timeline_ownership() -> Result<()> {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<AudioMasterClock>();
    assert_send_sync::<PlaybackClock>();

    let source = Arc::new(AudioMasterClock::new(SampleTime::new(0, 48_000)?));
    let publisher = {
        let source = Arc::clone(&source);
        thread::spawn(move || source.publish_sample(48_000))
    };
    assert_eq!(publisher.join().unwrap()?, AudioClockUpdate::Advanced);

    let clock =
        PlaybackClock::audio_master(RationalTime::new(0, Timebase::integer(48_000)?), source)?;
    assert_eq!(
        thread::spawn(move || clock.position()).join().unwrap()?,
        RationalTime::new(0, Timebase::integer(48_000)?)
    );

    Ok(())
}
