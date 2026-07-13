use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, Recoverability, Result};
use superi_core::ids::MediaId;
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_core::time::{Duration, FrameRate, RationalTime, Timebase};
use superi_media_io::backend::BackendDescriptor;
use superi_media_io::decode::{CpuVideoBuffer, VideoFormat, VideoFrame, VideoPlane};
use superi_media_io::demux::{
    BackendId, MetadataValue, SourceIdentity, SourceLocation, SourceRequest,
};
use superi_media_io::image_seq::{
    ImageSequenceBackend, ImageSequenceFrameAddress, ImageSequenceFrameReader,
    ImageSequenceFrameWriter, ImageSequenceInfo, ImageSequenceOutput, ImageSequenceOutputRequest,
    ImageSequenceSource, ImageSequenceTiming,
};

fn video_format() -> VideoFormat {
    VideoFormat::new(
        1,
        1,
        PixelFormat::Rgba8Unorm,
        ColorSpace::SRGB,
        AlphaMode::Straight,
    )
    .unwrap()
}

fn video_frame(address: ImageSequenceFrameAddress, value: u8) -> VideoFrame {
    let format = video_format();
    let plane = VideoPlane::new(Arc::from([value, value, value, 255]), 4, 1).unwrap();
    let buffer = Arc::new(CpuVideoBuffer::new(1, 1, format.pixel_format(), vec![plane]).unwrap());
    VideoFrame::new(
        format,
        address.presentation_time(),
        address.duration(),
        buffer,
    )
    .unwrap()
}

#[test]
fn sequence_timing_maps_logical_images_to_exact_file_and_presentation_coordinates() {
    let rate = FrameRate::FPS_24000_1001;
    let timing = ImageSequenceTiming::new(-1, 2, 3, rate)
        .unwrap()
        .with_presentation_start(RationalTime::new(48, rate.timebase()))
        .unwrap();

    let first = timing.address(0).unwrap();
    assert_eq!(first.image_number(), 0);
    assert_eq!(first.file_frame_number(), -1);
    assert_eq!(first.presentation_time().value(), 48);
    assert_eq!(first.presentation_time().timebase(), rate.timebase());
    assert_eq!(first.duration().value(), 1);
    assert_eq!(first.duration().timebase(), rate.timebase());

    let last = timing.address(2).unwrap();
    assert_eq!(last.file_frame_number(), 3);
    assert_eq!(last.presentation_time().value(), 50);
    assert_eq!(timing.duration().value(), 3);

    let doubled_rate = Timebase::new(48_000, 1_001).unwrap();
    assert_eq!(
        timing
            .image_number_for_time(RationalTime::new(98, doubled_rate))
            .unwrap(),
        1
    );
}

#[test]
fn sequence_timing_rejects_ambiguous_or_unrepresentable_coordinates() {
    let rate = FrameRate::FPS_24;

    for result in [
        ImageSequenceTiming::new(1, 0, 3, rate),
        ImageSequenceTiming::new(1, 1, 0, rate),
        ImageSequenceTiming::new(i64::MAX, 1, 2, rate),
    ] {
        let error = result.unwrap_err();
        assert_eq!(error.category(), ErrorCategory::InvalidInput);
    }

    let timing = ImageSequenceTiming::new(1001, 1, 3, rate).unwrap();
    assert_eq!(
        timing.address(3).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        timing
            .image_number_for_time(RationalTime::new(-1, rate.timebase()))
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );

    let half_frames = Timebase::integer(48).unwrap();
    assert_eq!(
        timing
            .image_number_for_time(RationalTime::new(1, half_frames))
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
}

struct MemoryReader {
    timestamp_offset: i64,
}

impl ImageSequenceFrameReader for MemoryReader {
    fn read_frame(&mut self, address: ImageSequenceFrameAddress) -> Result<VideoFrame> {
        let frame = video_frame(address, address.file_frame_number() as u8);
        if self.timestamp_offset == 0 {
            return Ok(frame);
        }
        let shifted = RationalTime::new(
            address.presentation_time().value() + self.timestamp_offset,
            address.presentation_time().timebase(),
        );
        VideoFrame::new(
            frame.format(),
            shifted,
            frame.duration(),
            frame.shared_buffer(),
        )
    }
}

fn sequence_info() -> ImageSequenceInfo {
    let timing = ImageSequenceTiming::new(1001, 2, 3, FrameRate::FPS_24)
        .unwrap()
        .with_presentation_start(RationalTime::new(10, FrameRate::FPS_24.timebase()))
        .unwrap();
    ImageSequenceInfo::new(
        SourceIdentity::new(MediaId::from_raw(42), "sha256:sequence-a").unwrap(),
        timing,
        video_format(),
    )
    .with_metadata("sequence.take", MetadataValue::Text("plate-main".into()))
    .unwrap()
}

#[test]
fn first_class_source_preserves_identity_and_validates_random_access_and_seek() {
    fn assert_send<T: Send>() {}
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send::<ImageSequenceSource>();
    assert_send_sync::<ImageSequenceInfo>();

    let mut source = ImageSequenceSource::new(
        sequence_info(),
        Box::new(MemoryReader {
            timestamp_offset: 0,
        }),
    );
    assert_eq!(source.info().identity().media_id(), MediaId::from_raw(42));
    assert_eq!(
        source.info().metadata().get("sequence.take"),
        Some(&MetadataValue::Text("plate-main".into()))
    );

    let random = source.read_frame(2).unwrap();
    assert_eq!(random.timestamp().value(), 12);
    assert_eq!(
        random.duration(),
        Duration::from_frames(1, FrameRate::FPS_24).unwrap()
    );

    let sought = source
        .seek(RationalTime::new(11, FrameRate::FPS_24.timebase()))
        .unwrap();
    assert_eq!(sought.timestamp().value(), 11);
    assert_eq!(sought.format(), video_format());
}

#[test]
fn source_rejects_backend_frames_that_change_declared_timing() {
    let mut source = ImageSequenceSource::new(
        sequence_info(),
        Box::new(MemoryReader {
            timestamp_offset: 1,
        }),
    );
    let error = source.read_frame(0).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::CorruptData);
    assert_eq!(error.recoverability(), Recoverability::Degraded);
    assert_eq!(error.contexts()[0].field("image_number"), Some("0"));
    assert_eq!(error.contexts()[0].field("file_frame"), Some("1001"));
}

struct MemoryWriter {
    addresses: Arc<Mutex<Vec<ImageSequenceFrameAddress>>>,
    fail_next: bool,
    fail_finish_next: bool,
}

impl ImageSequenceFrameWriter for MemoryWriter {
    fn write_frame(
        &mut self,
        address: ImageSequenceFrameAddress,
        _frame: &VideoFrame,
    ) -> Result<()> {
        if self.fail_next {
            self.fail_next = false;
            return Err(Error::new(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "temporary image writer failure",
            ));
        }
        self.addresses.lock().unwrap().push(address);
        Ok(())
    }

    fn finish(&mut self) -> Result<String> {
        if self.fail_finish_next {
            self.fail_finish_next = false;
            return Err(Error::new(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "temporary image writer finalization failure",
            ));
        }
        Ok("sha256:exported-sequence".into())
    }
}

fn output_request() -> ImageSequenceOutputRequest {
    ImageSequenceOutputRequest::new(
        MediaId::from_raw(77),
        PathBuf::from("renders/shot.%04d.exr"),
        sequence_info().timing(),
        video_format(),
    )
    .unwrap()
    .with_metadata("sequence.intent", MetadataValue::Text("final".into()))
    .unwrap()
}

#[test]
fn output_assigns_deterministic_addresses_and_only_advances_after_success() {
    fn assert_send<T: Send>() {}
    assert_send::<ImageSequenceOutput>();

    let addresses = Arc::new(Mutex::new(Vec::new()));
    let writer = MemoryWriter {
        addresses: Arc::clone(&addresses),
        fail_next: true,
        fail_finish_next: true,
    };
    let request = output_request();
    let timing = request.timing();
    let mut output = ImageSequenceOutput::new(request, Box::new(writer));

    let first_frame = video_frame(timing.address(0).unwrap(), 1);
    let error = output.write_frame(first_frame.clone()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unavailable);
    assert_eq!(error.contexts()[0].field("image_number"), Some("0"));
    assert_eq!(output.frames_written(), 0);

    let first = output.write_frame(first_frame).unwrap();
    assert_eq!(first.file_frame_number(), 1001);
    assert_eq!(output.frames_written(), 1);
    for image_number in 1..timing.frame_count() {
        let address = timing.address(image_number).unwrap();
        assert_eq!(
            output
                .write_frame(video_frame(address, image_number as u8))
                .unwrap(),
            address
        );
    }

    let finish_error = output.finish().unwrap_err();
    assert_eq!(finish_error.category(), ErrorCategory::Unavailable);
    assert_eq!(finish_error.recoverability(), Recoverability::Retryable);
    let completed = output.finish().unwrap();
    assert_eq!(output.finish().unwrap(), completed);
    let error = output
        .write_frame(video_frame(timing.address(0).unwrap(), 9))
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(output.frames_written(), timing.frame_count());
    assert_eq!(completed.identity().media_id(), MediaId::from_raw(77));
    assert_eq!(
        completed.identity().fingerprint(),
        "sha256:exported-sequence"
    );
    assert_eq!(completed.timing(), timing);
    assert_eq!(completed.format(), video_format());
    assert_eq!(
        completed.metadata().get("sequence.intent"),
        Some(&MetadataValue::Text("final".into()))
    );
    assert_eq!(
        addresses
            .lock()
            .unwrap()
            .iter()
            .map(|address| address.file_frame_number())
            .collect::<Vec<_>>(),
        [1001, 1003, 1005]
    );
}

#[test]
fn incomplete_output_cannot_publish_a_source_identity() {
    let addresses = Arc::new(Mutex::new(Vec::new()));
    let writer = MemoryWriter {
        addresses,
        fail_next: false,
        fail_finish_next: false,
    };
    let request = output_request();
    let first = request.timing().address(0).unwrap();
    let mut output = ImageSequenceOutput::new(request, Box::new(writer));
    output.write_frame(video_frame(first, 1)).unwrap();

    let error = output.finish().unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(error.contexts()[0].field("written"), Some("1"));
    assert_eq!(error.contexts()[0].field("expected"), Some("3"));
}

#[test]
fn output_rejects_invalid_destinations_and_mistimed_frames_without_advancing() {
    let request_error = ImageSequenceOutputRequest::new(
        MediaId::from_raw(77),
        PathBuf::new(),
        sequence_info().timing(),
        video_format(),
    )
    .unwrap_err();
    assert_eq!(request_error.category(), ErrorCategory::InvalidInput);

    let addresses = Arc::new(Mutex::new(Vec::new()));
    let writer = MemoryWriter {
        addresses: Arc::clone(&addresses),
        fail_next: false,
        fail_finish_next: false,
    };
    let request = output_request();
    let address = request.timing().address(0).unwrap();
    let valid = video_frame(address, 1);
    let mistimed = VideoFrame::new(
        valid.format(),
        RationalTime::new(valid.timestamp().value() + 1, valid.timestamp().timebase()),
        valid.duration(),
        valid.shared_buffer(),
    )
    .unwrap();
    let mut output = ImageSequenceOutput::new(request, Box::new(writer));
    let error = output.write_frame(mistimed).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(output.frames_written(), 0);
    assert!(addresses.lock().unwrap().is_empty());
}

struct MemoryBackend {
    descriptor: BackendDescriptor,
}

impl MemoryBackend {
    fn new() -> Self {
        Self {
            descriptor: BackendDescriptor::new(
                BackendId::new("image-memory").unwrap(),
                "Image memory backend",
            )
            .unwrap(),
        }
    }
}

impl ImageSequenceBackend for MemoryBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn open_source(&self, request: &SourceRequest) -> Result<ImageSequenceSource> {
        let fingerprint = "sha256:sequence-a";
        if request
            .expected_fingerprint()
            .is_some_and(|expected| expected != fingerprint)
        {
            return Err(Error::new(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "relinked image sequence does not match the expected content",
            ));
        }
        let mut info = sequence_info();
        info = ImageSequenceInfo::new(
            SourceIdentity::new(request.media_id(), fingerprint).unwrap(),
            info.timing(),
            info.format(),
        );
        Ok(ImageSequenceSource::new(
            info,
            Box::new(MemoryReader {
                timestamp_offset: 0,
            }),
        ))
    }

    fn create_output(&self, request: ImageSequenceOutputRequest) -> Result<ImageSequenceOutput> {
        Ok(ImageSequenceOutput::new(
            request,
            Box::new(MemoryWriter {
                addresses: Arc::new(Mutex::new(Vec::new())),
                fail_next: false,
                fail_finish_next: false,
            }),
        ))
    }
}

#[test]
fn public_backend_contract_drives_ingest_relink_playback_and_export() {
    let backend = MemoryBackend::new();
    assert_eq!(backend.descriptor().id().as_str(), "image-memory");

    let media_id = MediaId::from_raw(42);
    let original = SourceRequest::new(
        media_id,
        SourceLocation::Path(PathBuf::from("plates/a/shot.1001.exr")),
    );
    let mut source = backend.open_source(&original).unwrap();
    assert_eq!(source.info().identity().media_id(), media_id);
    assert_eq!(source.read_frame(0).unwrap().timestamp().value(), 10);

    let relink = SourceRequest::new(
        media_id,
        SourceLocation::Path(PathBuf::from("plates/b/shot.1001.exr")),
    )
    .with_expected_fingerprint(source.info().identity().fingerprint())
    .unwrap();
    let relinked = backend.open_source(&relink).unwrap();
    assert_eq!(relinked.info().identity(), source.info().identity());

    let mismatch = SourceRequest::new(media_id, original.location().clone())
        .with_expected_fingerprint("sha256:different")
        .unwrap();
    let Err(error) = backend.open_source(&mismatch) else {
        panic!("a relink fingerprint mismatch must fail")
    };
    assert_eq!(error.category(), ErrorCategory::Conflict);

    let request = output_request();
    let timing = request.timing();
    let mut output = backend.create_output(request).unwrap();
    for image_number in 0..timing.frame_count() {
        let address = timing.address(image_number).unwrap();
        output
            .write_frame(video_frame(address, image_number as u8))
            .unwrap();
    }
    assert_eq!(
        output.finish().unwrap().identity().fingerprint(),
        "sha256:exported-sequence"
    );
}
