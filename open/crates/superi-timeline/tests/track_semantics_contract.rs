use superi_core::error::ErrorCategory;
use superi_core::ids::{ClipId, TrackId};
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::time::{FrameRate, RationalTime, SampleTime, Timebase};
use superi_timeline::model::{
    AudioChannelRoute, AudioChannelTarget, AudioRecordContinuity, AudioRouteDestination,
    AudioRouting, AudioSourceContinuity, AudioSpan, AudioTrackSemantics, CaptionPurpose,
    CaptionTrackSemantics, DataSchema, DataTrackSemantics, LanguageTag, TrackKind, TrackSemantics,
    VideoCompositing, VideoTrackSemantics,
};

fn stereo_routing(destination: AudioRouteDestination) -> AudioRouting {
    AudioRouting::new(
        destination,
        ChannelLayout::stereo(),
        [
            AudioChannelRoute::new(
                ChannelPosition::FrontLeft,
                AudioChannelTarget::Channel(ChannelPosition::FrontLeft),
            ),
            AudioChannelRoute::new(
                ChannelPosition::FrontRight,
                AudioChannelTarget::Channel(ChannelPosition::FrontRight),
            ),
        ],
    )
    .unwrap()
}

fn stereo_audio(destination: AudioRouteDestination) -> AudioTrackSemantics {
    AudioTrackSemantics::new(48_000, ChannelLayout::stereo(), stereo_routing(destination)).unwrap()
}

#[test]
fn all_track_kinds_keep_their_semantics_explicit_and_editable() {
    let video = VideoTrackSemantics::new(FrameRate::FPS_24, VideoCompositing::Over);
    let audio = stereo_audio(AudioRouteDestination::Track(TrackId::from_raw(9)));
    let caption = CaptionTrackSemantics::new(
        Timebase::MILLISECONDS,
        LanguageTag::new("EN-us").unwrap(),
        CaptionPurpose::Captions,
    );
    let data = DataTrackSemantics::new(
        Timebase::integer(90_000).unwrap(),
        DataSchema::new("urn:scte:scte35:2013:xml", Some("splice_info")).unwrap(),
    );

    let tracks = [
        TrackSemantics::Video(video.clone()),
        TrackSemantics::Audio(audio.clone()),
        TrackSemantics::Caption(caption.clone()),
        TrackSemantics::Data(data.clone()),
    ];
    assert_eq!(
        tracks.each_ref().map(|track| track.kind()),
        [
            TrackKind::Video,
            TrackKind::Audio,
            TrackKind::Caption,
            TrackKind::Data,
        ]
    );
    assert_eq!(tracks[0].timebase(), FrameRate::FPS_24.timebase());
    assert_eq!(tracks[1].timebase(), Timebase::integer(48_000).unwrap());
    assert_eq!(tracks[2].timebase(), Timebase::MILLISECONDS);
    assert_eq!(tracks[3].timebase(), Timebase::integer(90_000).unwrap());

    assert_eq!(video.compositing(), VideoCompositing::Over);
    assert_eq!(audio.channel_layout(), &ChannelLayout::stereo());
    assert_eq!(
        audio.routing().destination(),
        AudioRouteDestination::Track(TrackId::from_raw(9))
    );
    assert_eq!(caption.language().as_str(), "en-us");
    assert_eq!(caption.purpose(), CaptionPurpose::Captions);
    assert_eq!(data.schema().scheme_id_uri(), "urn:scte:scte35:2013:xml");
    assert_eq!(data.schema().value(), Some("splice_info"));

    let replaced = audio
        .with_routing(stereo_routing(AudioRouteDestination::Main))
        .unwrap();
    assert_eq!(
        replaced.routing().destination(),
        AudioRouteDestination::Main
    );
    assert_eq!(replaced.channel_layout(), audio.channel_layout());
}

#[test]
fn audio_routing_preserves_ordered_channel_meaning_and_explicit_mutes() {
    let layout = ChannelLayout::new([
        ChannelPosition::FrontLeft,
        ChannelPosition::FrontRight,
        ChannelPosition::FrontCenter,
    ])
    .unwrap();
    let routing = AudioRouting::new(
        AudioRouteDestination::Main,
        ChannelLayout::stereo(),
        [
            AudioChannelRoute::new(
                ChannelPosition::FrontLeft,
                AudioChannelTarget::Channel(ChannelPosition::FrontLeft),
            ),
            AudioChannelRoute::new(
                ChannelPosition::FrontRight,
                AudioChannelTarget::Channel(ChannelPosition::FrontRight),
            ),
            AudioChannelRoute::new(ChannelPosition::FrontCenter, AudioChannelTarget::Muted),
        ],
    )
    .unwrap();
    let semantics = AudioTrackSemantics::new(48_000, layout.clone(), routing).unwrap();

    assert_eq!(semantics.channel_layout(), &layout);
    assert_eq!(semantics.routing().routes().len(), 3);
    assert_eq!(
        semantics.routing().routes()[2].target(),
        AudioChannelTarget::Muted
    );

    let incomplete = AudioRouting::new(
        AudioRouteDestination::Main,
        ChannelLayout::stereo(),
        [AudioChannelRoute::new(
            ChannelPosition::FrontLeft,
            AudioChannelTarget::Channel(ChannelPosition::FrontLeft),
        )],
    )
    .unwrap();
    let error = AudioTrackSemantics::new(48_000, layout.clone(), incomplete).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert!(error.message().contains("every source channel"));

    let bad_destination = AudioRouting::new(
        AudioRouteDestination::Main,
        ChannelLayout::mono(),
        [AudioChannelRoute::new(
            ChannelPosition::FrontCenter,
            AudioChannelTarget::Channel(ChannelPosition::FrontLeft),
        )],
    )
    .unwrap_err();
    assert_eq!(bad_destination.category(), ErrorCategory::InvalidInput);
}

#[test]
fn audio_spans_reshape_without_losing_sample_alignment_or_clip_identity() {
    let clip_id = ClipId::from_raw(17);
    let span = AudioSpan::new(
        clip_id,
        RationalTime::from_frames(24, FrameRate::FPS_24),
        SampleTime::new(96_000, 48_000).unwrap(),
        48_000,
    )
    .unwrap();

    assert_eq!(span.record_start().sample(), 48_000);
    assert_eq!(span.source_start().sample(), 96_000);
    assert_eq!(span.record_end().unwrap().sample(), 96_000);
    assert_eq!(span.source_end().unwrap().sample(), 144_000);
    assert_eq!(span.record_range().unwrap().duration().value(), 48_000);
    assert_eq!(span.source_range().unwrap().duration().value(), 48_000);

    let (left, right) = span.split_at(12_000).unwrap();
    assert_eq!(left.clip_id(), clip_id);
    assert_eq!(right.clip_id(), clip_id);
    assert_eq!(left.sample_count(), 12_000);
    assert_eq!(right.record_start().sample(), 60_000);
    assert_eq!(right.source_start().sample(), 108_000);

    let trimmed = right.trim_start(6_000).unwrap().trim_end(6_000).unwrap();
    assert_eq!(trimmed.clip_id(), clip_id);
    assert_eq!(trimmed.record_start().sample(), 66_000);
    assert_eq!(trimmed.source_start().sample(), 114_000);
    assert_eq!(trimmed.sample_count(), 24_000);

    let inexact = AudioSpan::new(
        clip_id,
        RationalTime::from_frames(1, FrameRate::FPS_24),
        SampleTime::new(0, 44_100).unwrap(),
        1_000,
    )
    .unwrap_err();
    assert_eq!(inexact.category(), ErrorCategory::InvalidInput);
    assert!(inexact.message().contains("sample boundary"));
}

#[test]
fn audio_continuity_reports_record_coverage_and_source_relationships() {
    let audio = stereo_audio(AudioRouteDestination::Main);
    let clip = ClipId::from_raw(31);
    let other_clip = ClipId::from_raw(32);
    let first = AudioSpan::new(
        clip,
        RationalTime::new(0, Timebase::integer(48_000).unwrap()),
        SampleTime::new(10_000, 48_000).unwrap(),
        48_000,
    )
    .unwrap();
    let second = AudioSpan::new(
        clip,
        RationalTime::new(48_000, Timebase::integer(48_000).unwrap()),
        SampleTime::new(58_000, 48_000).unwrap(),
        48_000,
    )
    .unwrap();
    let gap = AudioSpan::new(
        other_clip,
        RationalTime::new(108_000, Timebase::integer(48_000).unwrap()),
        SampleTime::new(0, 48_000).unwrap(),
        12_000,
    )
    .unwrap();

    let report = audio
        .audit_continuity(&[first.clone(), second.clone(), gap])
        .unwrap();
    assert!(!report.has_uninterrupted_record_coverage());
    assert_eq!(report.seams().len(), 2);
    assert_eq!(report.seams()[0].record(), AudioRecordContinuity::Seamless);
    assert_eq!(
        report.seams()[0].source(),
        AudioSourceContinuity::Continuous
    );
    assert_eq!(
        report.seams()[1].record(),
        AudioRecordContinuity::Gap {
            sample_count: 12_000
        }
    );
    assert_eq!(
        report.seams()[1].source(),
        AudioSourceContinuity::DifferentClip {
            left: clip,
            right: other_clip,
        }
    );

    let overlap = AudioSpan::new(
        other_clip,
        RationalTime::new(84_000, Timebase::integer(48_000).unwrap()),
        SampleTime::new(0, 48_000).unwrap(),
        12_000,
    )
    .unwrap();
    let overlap_report = audio.audit_continuity(&[first, second, overlap]).unwrap();
    assert!(overlap_report.has_uninterrupted_record_coverage());
    assert_eq!(
        overlap_report.seams()[1].record(),
        AudioRecordContinuity::Overlap {
            sample_count: 12_000
        }
    );
}

#[test]
fn caption_and_data_identifiers_reject_ambiguous_values() {
    assert_eq!(LanguageTag::new("x-PRIVATE").unwrap().as_str(), "x-private");
    assert_eq!(LanguageTag::new("i-KLINGON").unwrap().as_str(), "i-klingon");
    assert_eq!(
        LanguageTag::new("zh-Hant-TW").unwrap().as_str(),
        "zh-hant-tw"
    );
    assert_eq!(
        LanguageTag::new("en-US-u-ca-gregory").unwrap().as_str(),
        "en-us-u-ca-gregory"
    );

    for tag in [
        "",
        "a",
        "en--US",
        "en_Us",
        "1-en",
        "en-x",
        "en-u",
        "en-a-foo-a-bar",
        "sl-rozaj-rozaj",
    ] {
        let error = LanguageTag::new(tag).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::InvalidInput);
    }

    for schema in ["", "urn:example:\ncontrol"] {
        let error = DataSchema::new(schema, None).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::InvalidInput);
    }
}

#[test]
fn continuity_audit_reports_unrepresentable_seam_distances() {
    let audio = stereo_audio(AudioRouteDestination::Main);
    let timebase = Timebase::integer(48_000).unwrap();
    let first = AudioSpan::new(
        ClipId::from_raw(41),
        RationalTime::new(i64::MIN, timebase),
        SampleTime::new(0, 48_000).unwrap(),
        1,
    )
    .unwrap();
    let second = AudioSpan::new(
        ClipId::from_raw(42),
        RationalTime::new(i64::MAX - 1, timebase),
        SampleTime::new(0, 48_000).unwrap(),
        1,
    )
    .unwrap();

    let error = audio.audit_continuity(&[first, second]).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert!(error.message().contains("coordinate range"));
}
