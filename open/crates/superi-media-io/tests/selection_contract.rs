use std::sync::Arc;

use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::ids::MediaId;
use superi_core::time::Timebase;
use superi_media_io::demux::{
    CodecId, MetadataValue, Packet, PacketTiming, SourceIdentity, SourceInfo, StreamId, StreamInfo,
    StreamKind,
};
use superi_media_io::selection::{PairedStreamSelection, SelectedPacket, StreamPairRequest};

fn stream(id: u32, kind: StreamKind, codec: &str, timebase: u32) -> StreamInfo {
    StreamInfo::new(
        StreamId::new(id),
        kind,
        CodecId::new(codec).unwrap(),
        Timebase::integer(timebase).unwrap(),
    )
}

fn source(
    media_id: u128,
    fingerprint: &str,
    streams: impl IntoIterator<Item = StreamInfo>,
) -> SourceInfo {
    SourceInfo::new(
        SourceIdentity::new(MediaId::from_raw(media_id), fingerprint).unwrap(),
        streams.into_iter().collect(),
    )
    .unwrap()
}

fn packet(stream_id: u32, value: u8, timebase: u32) -> Packet {
    Packet::new(
        StreamId::new(stream_id),
        Arc::from([value]),
        PacketTiming::new(
            Timebase::integer(timebase).unwrap(),
            Some(12),
            Some(10),
            Some(2),
        )
        .unwrap(),
    )
    .with_keyframe(true)
    .with_metadata("container.offset", MetadataValue::Unsigned(88))
    .unwrap()
}

#[test]
fn explicit_pair_preserves_exact_stream_descriptors_for_decoder_consumers() {
    let selected_audio = stream(11, StreamKind::Audio, "pcm-s24le", 48_000)
        .with_metadata("audio.language", MetadataValue::Text("en-gb".into()))
        .unwrap()
        .with_metadata("audio.route", MetadataValue::Text("production-mix".into()))
        .unwrap();
    let info = source(
        41,
        "sha256:paired",
        [
            stream(20, StreamKind::Video, "av1", 24),
            stream(10, StreamKind::Audio, "aac", 48_000),
            stream(21, StreamKind::Video, "vp9", 30),
            selected_audio.clone(),
            stream(30, StreamKind::Subtitle, "webvtt", 1_000),
        ],
    );
    let request = StreamPairRequest::new(StreamId::new(21), StreamId::new(11));
    let selection = PairedStreamSelection::select(&info, request).unwrap();

    assert_eq!(request.video_stream_id(), StreamId::new(21));
    assert_eq!(request.audio_stream_id(), StreamId::new(11));
    assert_eq!(
        selection.source_identity().media_id(),
        MediaId::from_raw(41)
    );
    assert_eq!(selection.source_identity().fingerprint(), "sha256:paired");
    assert_eq!(selection.video_stream().id(), StreamId::new(21));
    assert_eq!(selection.video_stream().codec().as_str(), "vp9");
    assert_eq!(selection.video_stream().timebase().numerator(), 30);
    assert_eq!(selection.audio_stream(), &selected_audio);
    assert_eq!(selection.audio_stream().timebase().numerator(), 48_000);
    assert_eq!(
        selection.audio_stream().metadata().get("audio.language"),
        Some(&MetadataValue::Text("en-gb".into()))
    );
    assert_eq!(
        selection.audio_stream().metadata().get("audio.route"),
        Some(&MetadataValue::Text("production-mix".into()))
    );

    let video_config = selection.video_decoder_config();
    let audio_config = selection.audio_decoder_config();
    assert_eq!(video_config.stream(), selection.video_stream());
    assert_eq!(audio_config.stream(), selection.audio_stream());
}

#[test]
fn unambiguous_selection_never_uses_source_order_as_an_alternate_track_policy() {
    let unambiguous = source(
        42,
        "sha256:single",
        [
            stream(9, StreamKind::Subtitle, "webvtt", 1_000),
            stream(8, StreamKind::Audio, "opus", 48_000),
            stream(7, StreamKind::Video, "av1", 24),
        ],
    );
    let selected = PairedStreamSelection::select_unambiguous(&unambiguous).unwrap();
    assert_eq!(selected.video_stream().id(), StreamId::new(7));
    assert_eq!(selected.audio_stream().id(), StreamId::new(8));

    let ambiguous_audio = source(
        43,
        "sha256:alternates",
        [
            stream(1, StreamKind::Audio, "aac", 48_000),
            stream(2, StreamKind::Video, "h264", 24),
            stream(3, StreamKind::Audio, "pcm-s16le", 48_000),
        ],
    );
    let error = PairedStreamSelection::select_unambiguous(&ambiguous_audio).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(error.contexts()[0].field("stream_kind"), Some("audio"));
    assert_eq!(error.contexts()[0].field("candidate_count"), Some("2"));

    let video_only = source(
        44,
        "sha256:silent",
        [stream(1, StreamKind::Video, "av1", 24)],
    );
    let error = PairedStreamSelection::select_unambiguous(&video_only).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unsupported);
    assert_eq!(error.recoverability(), Recoverability::Degraded);
    assert_eq!(error.contexts()[0].field("stream_kind"), Some("audio"));
    assert_eq!(error.contexts()[0].field("candidate_count"), Some("0"));
}

#[test]
fn explicit_selection_rejects_missing_and_wrong_kind_streams_with_context() {
    let info = source(
        45,
        "sha256:validation",
        [
            stream(1, StreamKind::Video, "av1", 24),
            stream(2, StreamKind::Audio, "opus", 48_000),
            stream(3, StreamKind::Subtitle, "webvtt", 1_000),
        ],
    );

    let missing = PairedStreamSelection::select(
        &info,
        StreamPairRequest::new(StreamId::new(99), StreamId::new(2)),
    )
    .unwrap_err();
    assert_eq!(missing.category(), ErrorCategory::NotFound);
    assert_eq!(missing.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(missing.contexts()[0].field("stream_id"), Some("99"));
    assert_eq!(missing.contexts()[0].field("expected_kind"), Some("video"));

    let wrong_kind = PairedStreamSelection::select(
        &info,
        StreamPairRequest::new(StreamId::new(3), StreamId::new(2)),
    )
    .unwrap_err();
    assert_eq!(wrong_kind.category(), ErrorCategory::InvalidInput);
    assert_eq!(wrong_kind.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(wrong_kind.contexts()[0].field("stream_id"), Some("3"));
    assert_eq!(
        wrong_kind.contexts()[0].field("actual_kind"),
        Some("subtitle")
    );
}

#[test]
fn packet_routing_preserves_selected_and_unselected_packets_without_retiming() {
    let info = source(
        46,
        "sha256:routing",
        [
            stream(1, StreamKind::Video, "av1", 24),
            stream(2, StreamKind::Audio, "opus", 48_000),
            stream(3, StreamKind::Audio, "aac", 44_100),
        ],
    );
    let selection = PairedStreamSelection::select(
        &info,
        StreamPairRequest::new(StreamId::new(1), StreamId::new(2)),
    )
    .unwrap();

    let video = packet(1, 11, 24);
    let audio = packet(2, 22, 48_000);
    let alternate_audio = packet(3, 33, 44_100);

    let SelectedPacket::Video(video) = selection.route_packet(video) else {
        panic!("selected video packet was not routed to video")
    };
    assert_eq!(video.stream_id(), StreamId::new(1));
    assert_eq!(video.data(), [11]);
    assert_eq!(video.timing().presentation_time().unwrap().value(), 12);
    assert_eq!(video.timing().decode_time().unwrap().value(), 10);
    assert_eq!(video.timing().duration().unwrap().value(), 2);
    assert!(video.is_keyframe());
    assert_eq!(
        video.metadata().get("container.offset"),
        Some(&MetadataValue::Unsigned(88))
    );

    let SelectedPacket::Audio(audio) = selection.route_packet(audio) else {
        panic!("selected audio packet was not routed to audio")
    };
    assert_eq!(audio.stream_id(), StreamId::new(2));
    assert_eq!(audio.data(), [22]);
    assert_eq!(audio.timing().timebase().numerator(), 48_000);

    let SelectedPacket::Unselected(alternate_audio) = selection.route_packet(alternate_audio)
    else {
        panic!("alternate audio packet was not left unselected")
    };
    assert_eq!(alternate_audio.stream_id(), StreamId::new(3));
    assert_eq!(alternate_audio.data(), [33]);
    assert_eq!(alternate_audio.timing().timebase().numerator(), 44_100);
}

#[test]
fn relink_rebind_requires_the_same_content_and_compatible_stream_pair() {
    let original = source(
        47,
        "sha256:stable",
        [
            stream(1, StreamKind::Video, "av1", 24),
            stream(2, StreamKind::Audio, "opus", 48_000),
        ],
    );
    let selection = PairedStreamSelection::select_unambiguous(&original).unwrap();

    let moved = source(
        47,
        "sha256:stable",
        [
            stream(2, StreamKind::Audio, "opus", 48_000),
            stream(1, StreamKind::Video, "av1", 24),
        ],
    );
    let rebound = selection.rebind(&moved).unwrap();
    assert_eq!(rebound.video_stream().id(), StreamId::new(1));
    assert_eq!(rebound.audio_stream().id(), StreamId::new(2));

    let wrong_content = source(
        47,
        "sha256:different",
        [
            stream(1, StreamKind::Video, "av1", 24),
            stream(2, StreamKind::Audio, "opus", 48_000),
        ],
    );
    let error = selection.rebind(&wrong_content).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(
        error.contexts()[0].field("expected_fingerprint"),
        Some("sha256:stable")
    );
    assert_eq!(
        error.contexts()[0].field("actual_fingerprint"),
        Some("sha256:different")
    );

    let wrong_project_media = source(
        48,
        "sha256:stable",
        [
            stream(1, StreamKind::Video, "av1", 24),
            stream(2, StreamKind::Audio, "opus", 48_000),
        ],
    );
    let error = selection.rebind(&wrong_project_media).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    let expected_media_id = MediaId::from_raw(47).to_string();
    let actual_media_id = MediaId::from_raw(48).to_string();
    assert_eq!(
        error.contexts()[0].field("expected_media_id"),
        Some(expected_media_id.as_str())
    );
    assert_eq!(
        error.contexts()[0].field("actual_media_id"),
        Some(actual_media_id.as_str())
    );
}

#[test]
fn selection_values_are_safe_for_engine_and_background_owners() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<StreamPairRequest>();
    assert_send_sync::<PairedStreamSelection>();
    assert_send_sync::<SelectedPacket>();
}
