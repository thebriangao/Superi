//! First-class image-sequence source and output contracts.
//!
//! Logical image numbers are stable, zero-based positions in a sequence. File
//! frame numbers are signed labels used by a concrete image I/O implementation.
//! Keeping those domains separate preserves editorial timing when a sequence is
//! relinked to files with different local paths.

use std::path::{Path, PathBuf};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::MediaId;
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRounding};

use crate::backend::BackendDescriptor;
use crate::decode::{VideoFormat, VideoFrame};
use crate::demux::{MediaMetadata, MetadataValue, SourceIdentity, SourceRequest};

/// Exact timing and file-frame addressing for one image sequence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageSequenceTiming {
    first_file_frame: i64,
    file_frame_step: u32,
    frame_count: u64,
    frame_rate: FrameRate,
    presentation_start: RationalTime,
}

impl ImageSequenceTiming {
    /// Creates a sequence whose first logical image starts at presentation time zero.
    pub fn new(
        first_file_frame: i64,
        file_frame_step: u32,
        frame_count: u64,
        frame_rate: FrameRate,
    ) -> Result<Self> {
        if file_frame_step == 0 {
            return Err(invalid(
                "create_sequence_timing",
                "image sequence file-frame step must be greater than zero",
            ));
        }
        if frame_count == 0 {
            return Err(invalid(
                "create_sequence_timing",
                "image sequence must contain at least one logical image",
            ));
        }
        Duration::from_frames(frame_count, frame_rate)?;
        checked_file_frame(first_file_frame, file_frame_step, frame_count - 1)?;

        Ok(Self {
            first_file_frame,
            file_frame_step,
            frame_count,
            frame_rate,
            presentation_start: RationalTime::zero(frame_rate.timebase()),
        })
    }

    /// Sets the first presentation coordinate using exact timebase conversion.
    pub fn with_presentation_start(mut self, presentation_start: RationalTime) -> Result<Self> {
        let presentation_start =
            presentation_start.checked_rescale(self.frame_rate.timebase(), TimeRounding::Exact)?;
        presentation_start
            .value()
            .checked_add(self.frame_count as i64)
            .ok_or_else(|| {
                invalid(
                    "set_sequence_presentation_start",
                    "image sequence presentation range exceeds the supported coordinate range",
                )
            })?;
        self.presentation_start = presentation_start;
        Ok(self)
    }

    /// Returns the signed file-frame number of logical image zero.
    #[must_use]
    pub const fn first_file_frame(self) -> i64 {
        self.first_file_frame
    }

    /// Returns the positive step between adjacent file-frame numbers.
    #[must_use]
    pub const fn file_frame_step(self) -> u32 {
        self.file_frame_step
    }

    /// Returns the number of logical images in the sequence.
    #[must_use]
    pub const fn frame_count(self) -> u64 {
        self.frame_count
    }

    /// Returns the exact playback frame rate.
    #[must_use]
    pub const fn frame_rate(self) -> FrameRate {
        self.frame_rate
    }

    /// Returns the first presentation coordinate.
    #[must_use]
    pub const fn presentation_start(self) -> RationalTime {
        self.presentation_start
    }

    /// Returns the complete presentation duration.
    #[must_use]
    pub fn duration(self) -> Duration {
        Duration::from_frames(self.frame_count, self.frame_rate)
            .expect("validated image sequence duration")
    }

    /// Resolves a zero-based logical image into file and presentation coordinates.
    pub fn address(self, image_number: u64) -> Result<ImageSequenceFrameAddress> {
        if image_number >= self.frame_count {
            return Err(invalid_with_image(
                "resolve_sequence_address",
                "logical image number is outside the sequence",
                image_number,
            ));
        }
        let file_frame_number =
            checked_file_frame(self.first_file_frame, self.file_frame_step, image_number)?;
        let image_offset = i64::try_from(image_number).map_err(|_| {
            invalid_with_image(
                "resolve_sequence_address",
                "logical image number exceeds the supported coordinate range",
                image_number,
            )
        })?;
        let presentation_value = self
            .presentation_start
            .value()
            .checked_add(image_offset)
            .ok_or_else(|| {
                invalid_with_image(
                    "resolve_sequence_address",
                    "image presentation coordinate overflowed",
                    image_number,
                )
            })?;

        Ok(ImageSequenceFrameAddress {
            image_number,
            file_frame_number,
            presentation_time: RationalTime::new(presentation_value, self.frame_rate.timebase()),
            duration: Duration::from_frames(1, self.frame_rate)
                .expect("validated image sequence frame duration"),
        })
    }

    /// Resolves an exact presentation coordinate to a logical image number.
    pub fn image_number_for_time(self, presentation_time: RationalTime) -> Result<u64> {
        let presentation_time =
            presentation_time.checked_rescale(self.frame_rate.timebase(), TimeRounding::Exact)?;
        let offset = presentation_time
            .value()
            .checked_sub(self.presentation_start.value())
            .ok_or_else(|| {
                invalid(
                    "seek_sequence",
                    "image sequence presentation offset overflowed",
                )
            })?;
        let image_number = u64::try_from(offset).map_err(|_| {
            invalid(
                "seek_sequence",
                "presentation time occurs before the image sequence",
            )
        })?;
        if image_number >= self.frame_count {
            return Err(invalid_with_image(
                "seek_sequence",
                "presentation time occurs after the image sequence",
                image_number,
            ));
        }
        Ok(image_number)
    }
}

/// One independently addressable image in a sequence.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ImageSequenceFrameAddress {
    image_number: u64,
    file_frame_number: i64,
    presentation_time: RationalTime,
    duration: Duration,
}

impl ImageSequenceFrameAddress {
    /// Returns the zero-based logical image number.
    #[must_use]
    pub const fn image_number(self) -> u64 {
        self.image_number
    }

    /// Returns the signed frame number used by concrete image files.
    #[must_use]
    pub const fn file_frame_number(self) -> i64 {
        self.file_frame_number
    }

    /// Returns the exact presentation coordinate.
    #[must_use]
    pub const fn presentation_time(self) -> RationalTime {
        self.presentation_time
    }

    /// Returns the exact one-frame presentation duration.
    #[must_use]
    pub const fn duration(self) -> Duration {
        self.duration
    }
}

/// Immutable identity, timing, format, and metadata for an opened sequence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageSequenceInfo {
    identity: SourceIdentity,
    timing: ImageSequenceTiming,
    format: VideoFormat,
    metadata: MediaMetadata,
}

impl ImageSequenceInfo {
    /// Creates authoritative information for one sequence lifetime.
    #[must_use]
    pub fn new(identity: SourceIdentity, timing: ImageSequenceTiming, format: VideoFormat) -> Self {
        Self {
            identity,
            timing,
            format,
            metadata: MediaMetadata::new(),
        }
    }

    /// Adds preserved sequence metadata.
    pub fn with_metadata(mut self, key: impl Into<String>, value: MetadataValue) -> Result<Self> {
        self.metadata.insert(key, value)?;
        Ok(self)
    }

    /// Returns stable project and content identity.
    #[must_use]
    pub const fn identity(&self) -> &SourceIdentity {
        &self.identity
    }

    /// Returns authoritative sequence timing.
    #[must_use]
    pub const fn timing(&self) -> ImageSequenceTiming {
        self.timing
    }

    /// Returns the constant decoded representation declared for this sequence.
    #[must_use]
    pub const fn format(&self) -> VideoFormat {
        self.format
    }

    /// Returns preserved sequence metadata.
    #[must_use]
    pub const fn metadata(&self) -> &MediaMetadata {
        &self.metadata
    }
}

/// Backend-owned random-access image reader.
pub trait ImageSequenceFrameReader: Send {
    /// Reads and decodes one independently addressable image.
    fn read_frame(&mut self, address: ImageSequenceFrameAddress) -> Result<VideoFrame>;
}

/// Open first-class image sequence with backend-independent validation.
pub struct ImageSequenceSource {
    info: ImageSequenceInfo,
    reader: Box<dyn ImageSequenceFrameReader>,
}

impl ImageSequenceSource {
    /// Creates a source around authoritative sequence information and a reader.
    #[must_use]
    pub fn new(info: ImageSequenceInfo, reader: Box<dyn ImageSequenceFrameReader>) -> Self {
        Self { info, reader }
    }

    /// Returns immutable identity, timing, format, and metadata.
    #[must_use]
    pub const fn info(&self) -> &ImageSequenceInfo {
        &self.info
    }

    /// Reads one zero-based logical image and validates the backend result.
    pub fn read_frame(&mut self, image_number: u64) -> Result<VideoFrame> {
        let address = self.info.timing.address(image_number)?;
        let frame = self
            .reader
            .read_frame(address)
            .map_err(|error| error.with_context(frame_context("read_sequence_frame", address)))?;
        validate_frame(&frame, self.info.format, address, "read_sequence_frame")?;
        Ok(frame)
    }

    /// Seeks to an exact presentation coordinate and reads that logical image.
    pub fn seek(&mut self, presentation_time: RationalTime) -> Result<VideoFrame> {
        let image_number = self.info.timing.image_number_for_time(presentation_time)?;
        self.read_frame(image_number)
    }
}

/// Requested identity, destination, timing, and format for one sequence output.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImageSequenceOutputRequest {
    media_id: MediaId,
    destination: PathBuf,
    timing: ImageSequenceTiming,
    format: VideoFormat,
    metadata: MediaMetadata,
}

impl ImageSequenceOutputRequest {
    /// Creates an output request for a concrete backend destination.
    pub fn new(
        media_id: MediaId,
        destination: PathBuf,
        timing: ImageSequenceTiming,
        format: VideoFormat,
    ) -> Result<Self> {
        if destination.as_os_str().is_empty() {
            return Err(invalid(
                "create_sequence_output_request",
                "image sequence output destination must not be empty",
            ));
        }
        Ok(Self {
            media_id,
            destination,
            timing,
            format,
            metadata: MediaMetadata::new(),
        })
    }

    /// Adds metadata that must be preserved by the completed sequence.
    pub fn with_metadata(mut self, key: impl Into<String>, value: MetadataValue) -> Result<Self> {
        self.metadata.insert(key, value)?;
        Ok(self)
    }

    /// Returns persistent project media identity for the output.
    #[must_use]
    pub const fn media_id(&self) -> MediaId {
        self.media_id
    }

    /// Returns the backend-defined filesystem destination.
    #[must_use]
    pub fn destination(&self) -> &Path {
        &self.destination
    }

    /// Returns exact output timing.
    #[must_use]
    pub const fn timing(&self) -> ImageSequenceTiming {
        self.timing
    }

    /// Returns the required decoded representation for every output image.
    #[must_use]
    pub const fn format(&self) -> VideoFormat {
        self.format
    }

    /// Returns metadata to attach to the completed sequence.
    #[must_use]
    pub const fn metadata(&self) -> &MediaMetadata {
        &self.metadata
    }
}

/// Backend-owned image writer beneath the deterministic output lifecycle.
pub trait ImageSequenceFrameWriter: Send {
    /// Writes one validated frame to its concrete file-frame destination.
    fn write_frame(&mut self, address: ImageSequenceFrameAddress, frame: &VideoFrame)
        -> Result<()>;

    /// Publishes the completed output and returns an opaque content fingerprint.
    fn finish(&mut self) -> Result<String>;
}

/// Sequential image-sequence output with backend-independent validation.
pub struct ImageSequenceOutput {
    request: ImageSequenceOutputRequest,
    writer: Box<dyn ImageSequenceFrameWriter>,
    frames_written: u64,
    completed: Option<ImageSequenceInfo>,
}

impl ImageSequenceOutput {
    /// Creates an output around a validated request and concrete writer.
    #[must_use]
    pub fn new(
        request: ImageSequenceOutputRequest,
        writer: Box<dyn ImageSequenceFrameWriter>,
    ) -> Self {
        Self {
            request,
            writer,
            frames_written: 0,
            completed: None,
        }
    }

    /// Returns immutable output configuration.
    #[must_use]
    pub const fn request(&self) -> &ImageSequenceOutputRequest {
        &self.request
    }

    /// Returns the number of frames durably accepted by the writer.
    #[must_use]
    pub const fn frames_written(&self) -> u64 {
        self.frames_written
    }

    /// Writes the next logical image and returns its assigned address.
    pub fn write_frame(&mut self, frame: VideoFrame) -> Result<ImageSequenceFrameAddress> {
        if self.completed.is_some() {
            return Err(output_conflict(
                "write_sequence_frame",
                "image sequence output is already complete",
                self.frames_written,
                self.request.timing.frame_count,
            ));
        }
        if self.frames_written >= self.request.timing.frame_count {
            return Err(output_conflict(
                "write_sequence_frame",
                "image sequence output already contains every declared frame",
                self.frames_written,
                self.request.timing.frame_count,
            ));
        }
        let address = self.request.timing.address(self.frames_written)?;
        validate_output_frame(&frame, self.request.format, address)?;
        self.writer
            .write_frame(address, &frame)
            .map_err(|error| error.with_context(frame_context("write_sequence_frame", address)))?;
        self.frames_written += 1;
        Ok(address)
    }

    /// Publishes a complete sequence and returns reopen-ready source information.
    ///
    /// Retryable backend failures preserve the session. Repeated calls after a
    /// successful publish return the same source information without finishing
    /// the writer again.
    pub fn finish(&mut self) -> Result<ImageSequenceInfo> {
        if let Some(info) = &self.completed {
            return Ok(info.clone());
        }
        if self.frames_written != self.request.timing.frame_count {
            return Err(output_conflict(
                "finish_sequence_output",
                "image sequence output is incomplete",
                self.frames_written,
                self.request.timing.frame_count,
            ));
        }
        let fingerprint = self.writer.finish().map_err(|error| {
            error.with_context(
                ErrorContext::new("superi-media-io.image-sequence", "finish_sequence_output")
                    .with_field(
                        "destination",
                        self.request.destination.display().to_string(),
                    ),
            )
        })?;
        let identity = SourceIdentity::new(self.request.media_id, fingerprint)?;
        let info = ImageSequenceInfo {
            identity,
            timing: self.request.timing,
            format: self.request.format,
            metadata: self.request.metadata.clone(),
        };
        self.completed = Some(info.clone());
        Ok(info)
    }
}

/// Image-subsystem backend that opens and writes first-class sequences.
///
/// Capability selection and fallback policy are owned by the engine registry.
/// Implementations must return shared errors rather than silently substituting
/// another backend.
pub trait ImageSequenceBackend: Send + Sync {
    /// Returns stable backend identity for diagnostics and capability reporting.
    fn descriptor(&self) -> &BackendDescriptor;

    /// Opens an image sequence for ingest, playback, or relinking.
    fn open_source(&self, request: &SourceRequest) -> Result<ImageSequenceSource>;

    /// Creates a deterministic output sequence.
    fn create_output(&self, request: ImageSequenceOutputRequest) -> Result<ImageSequenceOutput>;
}

fn validate_frame(
    frame: &VideoFrame,
    expected_format: VideoFormat,
    address: ImageSequenceFrameAddress,
    operation: &'static str,
) -> Result<()> {
    if frame.format() != expected_format {
        return Err(inconsistent_frame(
            operation,
            "image sequence frame format changed within the source lifetime",
            address,
        ));
    }
    if frame.timestamp() != address.presentation_time()
        || frame.timestamp().timebase() != address.presentation_time().timebase()
    {
        return Err(inconsistent_frame(
            operation,
            "image sequence frame timestamp does not match its logical address",
            address,
        ));
    }
    if frame.duration() != address.duration()
        || frame.duration().timebase() != address.duration().timebase()
    {
        return Err(inconsistent_frame(
            operation,
            "image sequence frame duration does not match its logical address",
            address,
        ));
    }
    Ok(())
}

fn validate_output_frame(
    frame: &VideoFrame,
    expected_format: VideoFormat,
    address: ImageSequenceFrameAddress,
) -> Result<()> {
    if frame.format() != expected_format {
        return Err(invalid_output_frame(
            "output frame format does not match the sequence request",
            address,
        ));
    }
    if frame.timestamp() != address.presentation_time()
        || frame.timestamp().timebase() != address.presentation_time().timebase()
    {
        return Err(invalid_output_frame(
            "output frame timestamp does not match its assigned sequence address",
            address,
        ));
    }
    if frame.duration() != address.duration()
        || frame.duration().timebase() != address.duration().timebase()
    {
        return Err(invalid_output_frame(
            "output frame duration does not match its assigned sequence address",
            address,
        ));
    }
    Ok(())
}

fn checked_file_frame(first: i64, step: u32, image_number: u64) -> Result<i64> {
    let offset = i128::from(step)
        .checked_mul(i128::from(image_number))
        .and_then(|value| value.checked_add(i128::from(first)))
        .ok_or_else(|| {
            invalid_with_image(
                "resolve_file_frame",
                "image sequence file-frame number overflowed",
                image_number,
            )
        })?;
    i64::try_from(offset).map_err(|_| {
        invalid_with_image(
            "resolve_file_frame",
            "image sequence file-frame number exceeds the supported range",
            image_number,
        )
    })
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(
        "superi-media-io.image-sequence",
        operation,
    ))
}

fn invalid_with_image(operation: &'static str, message: &'static str, image_number: u64) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(
        ErrorContext::new("superi-media-io.image-sequence", operation)
            .with_field("image_number", image_number.to_string()),
    )
}

fn inconsistent_frame(
    operation: &'static str,
    message: &'static str,
    address: ImageSequenceFrameAddress,
) -> Error {
    Error::new(
        ErrorCategory::CorruptData,
        Recoverability::Degraded,
        message,
    )
    .with_context(frame_context(operation, address))
}

fn frame_context(operation: &'static str, address: ImageSequenceFrameAddress) -> ErrorContext {
    ErrorContext::new("superi-media-io.image-sequence", operation)
        .with_field("image_number", address.image_number.to_string())
        .with_field("file_frame", address.file_frame_number.to_string())
}

fn invalid_output_frame(message: &'static str, address: ImageSequenceFrameAddress) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(frame_context("write_sequence_frame", address))
}

fn output_conflict(
    operation: &'static str,
    message: &'static str,
    written: u64,
    expected: u64,
) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(
        ErrorContext::new("superi-media-io.image-sequence", operation)
            .with_field("written", written.to_string())
            .with_field("expected", expected.to_string()),
    )
}
