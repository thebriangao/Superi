use std::sync::Arc;
use std::thread;

use superi_concurrency::backpressure::{
    bounded_handoff, BackpressureConfig, HandoffReceiver, HandoffSender, PipelineRoute,
    PipelineStage,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::color_space::ColorSpace;
use superi_core::error::ErrorCategory;
use superi_core::pixel::{AlphaMode, ChannelLayout, PixelFormat, SampleFormat};
use superi_core::time::{Duration, RationalTime, SampleTime, Timebase};
use superi_media_io::audio_io::{AudioBlock, AudioFormat, AudioPlane};
use superi_media_io::decode::{CpuVideoBuffer, VideoFormat, VideoFrame, VideoPlane};
use superi_media_io::demux::MetadataValue;

#[test]
fn configuration_names_every_stage_and_rejects_unbounded_or_cyclic_routes() {
    let expected = [
        (PipelineStage::Decode, "decode"),
        (PipelineStage::Graph, "graph"),
        (PipelineStage::Cache, "cache"),
        (PipelineStage::Audio, "audio"),
        (PipelineStage::Viewport, "viewport"),
        (PipelineStage::Export, "export"),
    ];
    assert_eq!(PipelineStage::ALL, expected.map(|(stage, _)| stage));
    for (stage, code) in expected {
        assert_eq!(stage.code(), code);
        assert_eq!(PipelineStage::from_code(code), Some(stage));
    }
    assert_eq!(PipelineStage::from_code("unknown"), None);

    let route = PipelineRoute::new(PipelineStage::Decode, PipelineStage::Graph).unwrap();
    assert_eq!(route.producer(), PipelineStage::Decode);
    assert_eq!(route.consumer(), PipelineStage::Graph);
    assert_eq!(route.to_string(), "decode -> graph");

    let cyclic = PipelineRoute::new(PipelineStage::Cache, PipelineStage::Cache).unwrap_err();
    assert_eq!(cyclic.category(), ErrorCategory::InvalidInput);
    let unbounded = BackpressureConfig::new(route, 0).unwrap_err();
    assert_eq!(unbounded.category(), ErrorCategory::InvalidInput);

    let config = BackpressureConfig::new(route, 3).unwrap();
    assert_eq!(config.route(), route);
    assert_eq!(config.capacity(), 3);
}

#[test]
fn saturation_returns_video_ownership_without_changing_media_meaning() {
    let route = PipelineRoute::new(PipelineStage::Decode, PipelineStage::Graph).unwrap();
    let (sender, receiver) = bounded_handoff(BackpressureConfig::new(route, 1).unwrap());
    let first = video_frame(11, 7);
    let second = video_frame(29, 8);
    let second_storage = second.shared_buffer();

    sender.try_send(first).unwrap();
    let full = sender.try_send(second).unwrap_err();
    assert_eq!(full.route(), route);
    assert_eq!(full.capacity(), 1);
    assert_eq!(full.code(), "capacity_reached");
    let second = full.into_item();

    assert_video(&second, 29, 8);
    assert!(Arc::ptr_eq(&second_storage, &second.shared_buffer()));
    assert_video(&receiver.try_receive().unwrap(), 11, 7);
    assert!(receiver.try_receive().is_none());
}

#[test]
fn audio_handoff_preserves_sample_precision_channel_order_and_metadata() {
    let route = PipelineRoute::new(PipelineStage::Decode, PipelineStage::Audio).unwrap();
    let (sender, receiver) = bounded_handoff(BackpressureConfig::new(route, 2).unwrap());
    let audio = audio_block();
    let expected = audio.clone();

    sender.try_send(audio).unwrap();
    let received = receiver.try_receive().unwrap();
    assert_eq!(received, expected);
    assert_eq!(
        received.timestamp(),
        SampleTime::new(96_001, 48_000).unwrap()
    );
    assert_eq!(received.format().sample_format(), SampleFormat::F32Planar);
    assert_eq!(received.format().channel_layout(), &ChannelLayout::stereo());
    assert_eq!(
        received.metadata().get("source.channel-order"),
        Some(&MetadataValue::Text("left,right".to_owned()))
    );
}

#[test]
fn audio_domain_uses_a_preallocated_scalar_handoff_through_nonblocking_apis() {
    let route = PipelineRoute::new(PipelineStage::Decode, PipelineStage::Audio).unwrap();
    let (sender, receiver) = bounded_handoff(BackpressureConfig::new(route, 1).unwrap());
    let audio_domain = ExecutionDomain::Audio.enter_current().unwrap();

    sender.try_send(96_001_u64).unwrap();
    assert_eq!(receiver.try_receive(), Some(96_001));

    drop(audio_domain);
}

#[test]
fn saturated_export_does_not_consume_viewport_or_audio_capacity() {
    let graph_cache = PipelineRoute::new(PipelineStage::Graph, PipelineStage::Cache).unwrap();
    let cache_audio = PipelineRoute::new(PipelineStage::Cache, PipelineStage::Audio).unwrap();
    let cache_viewport = PipelineRoute::new(PipelineStage::Cache, PipelineStage::Viewport).unwrap();
    let cache_export = PipelineRoute::new(PipelineStage::Cache, PipelineStage::Export).unwrap();
    let (graph_sender, graph_receiver) =
        bounded_handoff(BackpressureConfig::new(graph_cache, 1).unwrap());
    let (audio_sender, audio_receiver) =
        bounded_handoff(BackpressureConfig::new(cache_audio, 1).unwrap());
    let (viewport_sender, viewport_receiver) =
        bounded_handoff(BackpressureConfig::new(cache_viewport, 1).unwrap());
    let (export_sender, export_receiver) =
        bounded_handoff(BackpressureConfig::new(cache_export, 1).unwrap());

    export_sender.try_send("export-0").unwrap();
    assert_eq!(
        export_sender.try_send("export-1").unwrap_err().into_item(),
        "export-1"
    );
    graph_sender.try_send("graph-0").unwrap();
    audio_sender.try_send("audio-0").unwrap();
    viewport_sender.try_send("viewport-0").unwrap();

    assert_eq!(graph_receiver.try_receive(), Some("graph-0"));
    assert_eq!(audio_receiver.try_receive(), Some("audio-0"));
    assert_eq!(viewport_receiver.try_receive(), Some("viewport-0"));
    assert_eq!(export_receiver.try_receive(), Some("export-0"));
}

#[test]
fn snapshots_expose_capacity_without_reserving_or_dropping_items() {
    let route = PipelineRoute::new(PipelineStage::Cache, PipelineStage::Viewport).unwrap();
    let (sender, receiver) = bounded_handoff(BackpressureConfig::new(route, 2).unwrap());

    let empty = sender.snapshot();
    assert_eq!(empty.route(), route);
    assert_eq!(empty.capacity(), 2);
    assert_eq!(empty.queued_items(), 0);
    assert_eq!(empty.remaining_capacity(), 2);
    assert!(empty.is_empty());
    assert!(!empty.is_full());

    sender.try_send(1).unwrap();
    sender.try_send(2).unwrap();
    let full = receiver.snapshot();
    assert_eq!(full.queued_items(), 2);
    assert_eq!(full.remaining_capacity(), 0);
    assert!(full.is_full());
    assert_eq!(receiver.try_receive(), Some(1));
    assert_eq!(receiver.try_receive(), Some(2));
}

#[test]
fn cloned_endpoints_deliver_concurrent_work_once_with_a_hard_bound() {
    const PRODUCERS: usize = 4;
    const ITEMS_PER_PRODUCER: usize = 128;
    const TOTAL: usize = PRODUCERS * ITEMS_PER_PRODUCER;

    let route = PipelineRoute::new(PipelineStage::Decode, PipelineStage::Graph).unwrap();
    let (sender, receiver) = bounded_handoff(BackpressureConfig::new(route, 7).unwrap());
    let consumer = thread::spawn(move || {
        let mut received = Vec::with_capacity(TOTAL);
        while received.len() < TOTAL {
            if let Some(value) = receiver.try_receive() {
                received.push(value);
            } else {
                thread::yield_now();
            }
        }
        received
    });

    let producers = (0..PRODUCERS)
        .map(|producer| {
            let sender = sender.clone();
            thread::spawn(move || {
                for offset in 0..ITEMS_PER_PRODUCER {
                    let mut item = producer * ITEMS_PER_PRODUCER + offset;
                    loop {
                        match sender.try_send(item) {
                            Ok(()) => break,
                            Err(full) => {
                                item = full.into_item();
                                thread::yield_now();
                            }
                        }
                    }
                }
            })
        })
        .collect::<Vec<_>>();
    for producer in producers {
        producer.join().unwrap();
    }

    let mut received = consumer.join().unwrap();
    received.sort_unstable();
    assert_eq!(received, (0..TOTAL).collect::<Vec<_>>());
    assert!(sender.snapshot().is_empty());
}

#[test]
fn handoff_endpoints_are_send_and_sync_for_sendable_media() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<HandoffSender<VideoFrame>>();
    assert_send_sync::<HandoffReceiver<VideoFrame>>();
}

fn video_frame(value: u8, frame: i64) -> VideoFrame {
    let format = VideoFormat::new(
        2,
        2,
        PixelFormat::Rgba8Unorm,
        ColorSpace::SRGB,
        AlphaMode::Straight,
    )
    .unwrap();
    let plane = VideoPlane::new(Arc::from(vec![value; 16]), 8, 2).unwrap();
    let buffer = Arc::new(CpuVideoBuffer::new(2, 2, PixelFormat::Rgba8Unorm, vec![plane]).unwrap());
    let timebase = Timebase::integer(24).unwrap();
    VideoFrame::new(
        format,
        RationalTime::new(frame, timebase),
        Duration::new(1, timebase).unwrap(),
        buffer,
    )
    .unwrap()
    .with_metadata("source.frame", MetadataValue::Signed(frame))
    .unwrap()
}

fn assert_video(frame: &VideoFrame, value: u8, timestamp: i64) {
    assert_eq!(frame.timestamp().value(), timestamp);
    assert_eq!(frame.format().pixel_format(), PixelFormat::Rgba8Unorm);
    assert_eq!(frame.format().color_space(), ColorSpace::SRGB);
    assert_eq!(frame.format().alpha_mode(), AlphaMode::Straight);
    assert_eq!(
        frame.metadata().get("source.frame"),
        Some(&MetadataValue::Signed(timestamp))
    );
    let buffer = frame
        .buffer()
        .as_any()
        .downcast_ref::<CpuVideoBuffer>()
        .unwrap();
    assert_eq!(buffer.planes()[0].bytes(), &[value; 16]);
}

fn audio_block() -> AudioBlock {
    let format =
        AudioFormat::new(48_000, SampleFormat::F32Planar, ChannelLayout::stereo()).unwrap();
    let left = [0.25_f32, -0.5]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    let right = [0.75_f32, -1.0]
        .into_iter()
        .flat_map(f32::to_le_bytes)
        .collect::<Vec<_>>();
    AudioBlock::new(
        format,
        SampleTime::new(96_001, 48_000).unwrap(),
        2,
        vec![AudioPlane::new(left.into()), AudioPlane::new(right.into())],
    )
    .unwrap()
    .with_metadata(
        "source.channel-order",
        MetadataValue::Text("left,right".to_owned()),
    )
    .unwrap()
}
