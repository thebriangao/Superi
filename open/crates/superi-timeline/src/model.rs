//! Native semantic values shared by editorial tracks.
//!
//! This module owns the distinctions between video, audio, caption, and data
//! tracks. Project, timeline, track, and clip containers are separate editorial
//! objects. The semantic values here are designed to be embedded by those
//! objects without creating another identity or time model.

use std::collections::{BTreeMap, BTreeSet};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use superi_core::ids::{
    CaptionId, ClipId, GapId, GeneratorId, MarkerId, MediaId, ProjectId, SmartCollectionId,
    TimelineId, TrackId, TransitionId,
};
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::time::{
    Duration, FrameRate, RationalTime, SampleTime, TimeRange, TimeRounding, Timebase,
};
use superi_graph::node::GraphColorMetadata;

use crate::edit_state::{
    SelectionExpansion, SelectionUpdate, TimelineEditState, MAX_TRACK_HEIGHT, MIN_TRACK_HEIGHT,
};
use crate::markers::{
    Marker, MetadataOwner, SnapMatch, SnapRequest, TimelineAnnotations, TimelineMetadata,
};
pub use crate::media::LinkedMediaReference;
use crate::media::MediaLibrary;
use crate::multicam::{find_clip, MulticamClip, MulticamSource};
use crate::retime::{ClipTimeMap, MappedSourceTime, RetimeMode, RetimeResolution};

/// The semantic media class of an editorial track.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum TrackKind {
    /// Ordered visual content.
    Video,
    /// Sample-clocked audio content.
    Audio,
    /// Timed human-readable text.
    Caption,
    /// Timed structured events or metadata.
    Data,
}

/// Complete embeddable semantics for one editorial track.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TrackSemantics {
    /// Visual track behavior.
    Video(VideoTrackSemantics),
    /// Audio track behavior.
    Audio(AudioTrackSemantics),
    /// Timed-text behavior.
    Caption(CaptionTrackSemantics),
    /// Timed-data behavior.
    Data(DataTrackSemantics),
}

impl TrackSemantics {
    /// Returns the stable semantic class.
    #[must_use]
    pub const fn kind(&self) -> TrackKind {
        match self {
            Self::Video(_) => TrackKind::Video,
            Self::Audio(_) => TrackKind::Audio,
            Self::Caption(_) => TrackKind::Caption,
            Self::Data(_) => TrackKind::Data,
        }
    }

    /// Returns the exact clock used for direct track edits.
    #[must_use]
    pub fn timebase(&self) -> Timebase {
        match self {
            Self::Video(semantics) => semantics.frame_rate().timebase(),
            Self::Audio(semantics) => semantics.timebase(),
            Self::Caption(semantics) => semantics.timebase(),
            Self::Data(semantics) => semantics.timebase(),
        }
    }
}

/// How a video track contributes to visual tracks below it.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum VideoCompositing {
    /// Alpha-composite the track over lower visual tracks.
    Over,
    /// Replace lower visual tracks where this track has content.
    Replace,
}

/// Exact edit-clock and compositing semantics for a video track.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VideoTrackSemantics {
    frame_rate: FrameRate,
    compositing: VideoCompositing,
}

impl VideoTrackSemantics {
    /// Creates video semantics with an exact frame rate.
    #[must_use]
    pub const fn new(frame_rate: FrameRate, compositing: VideoCompositing) -> Self {
        Self {
            frame_rate,
            compositing,
        }
    }

    /// Returns the exact visual edit rate.
    #[must_use]
    pub const fn frame_rate(&self) -> FrameRate {
        self.frame_rate
    }

    /// Returns how this track contributes to lower visual tracks.
    #[must_use]
    pub const fn compositing(&self) -> VideoCompositing {
        self.compositing
    }

    /// Returns a copy with directly replaced compositing behavior.
    #[must_use]
    pub const fn with_compositing(&self, compositing: VideoCompositing) -> Self {
        Self::new(self.frame_rate, compositing)
    }
}

/// The typed object that receives a routed audio track.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum AudioRouteDestination {
    /// The timeline's main output.
    Main,
    /// Another editorial track, normally an audio bus.
    Track(TrackId),
}

/// The explicit destination for one source channel.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum AudioChannelTarget {
    /// Route into the named output channel meaning.
    Channel(ChannelPosition),
    /// Intentionally suppress this source channel.
    Muted,
}

/// One source-channel routing decision.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AudioChannelRoute {
    source: ChannelPosition,
    target: AudioChannelTarget,
}

impl AudioChannelRoute {
    /// Creates an explicit source-to-destination decision.
    #[must_use]
    pub const fn new(source: ChannelPosition, target: AudioChannelTarget) -> Self {
        Self { source, target }
    }

    /// Returns the source channel meaning.
    #[must_use]
    pub const fn source(self) -> ChannelPosition {
        self.source
    }

    /// Returns the destination channel or explicit mute decision.
    #[must_use]
    pub const fn target(self) -> AudioChannelTarget {
        self.target
    }
}

/// Ordered, inspectable routing intent for an audio track.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioRouting {
    destination: AudioRouteDestination,
    destination_layout: ChannelLayout,
    routes: Box<[AudioChannelRoute]>,
}

impl AudioRouting {
    /// Creates validated routing intent.
    ///
    /// Source coverage is validated when the routing is attached to
    /// [`AudioTrackSemantics`]. This constructor verifies that source decisions
    /// are unique and every non-muted target exists in the destination layout.
    pub fn new<I>(
        destination: AudioRouteDestination,
        destination_layout: ChannelLayout,
        routes: I,
    ) -> Result<Self>
    where
        I: IntoIterator<Item = AudioChannelRoute>,
    {
        let routes: Vec<_> = routes.into_iter().collect();
        if routes.is_empty() {
            return Err(invalid_model(
                "create_audio_routing",
                "audio routing must contain at least one source decision",
            ));
        }
        for (index, route) in routes.iter().enumerate() {
            if routes[..index]
                .iter()
                .any(|candidate| candidate.source == route.source)
            {
                return Err(invalid_model(
                    "create_audio_routing",
                    "audio routing contains a duplicate source channel",
                ));
            }
            if let AudioChannelTarget::Channel(position) = route.target {
                if !destination_layout.positions().contains(&position) {
                    return Err(invalid_model(
                        "create_audio_routing",
                        "audio route target is absent from the destination layout",
                    ));
                }
            }
        }

        Ok(Self {
            destination,
            destination_layout,
            routes: routes.into_boxed_slice(),
        })
    }

    /// Returns the typed route destination.
    #[must_use]
    pub const fn destination(&self) -> AudioRouteDestination {
        self.destination
    }

    /// Returns the ordered output channel meanings.
    #[must_use]
    pub const fn destination_layout(&self) -> &ChannelLayout {
        &self.destination_layout
    }

    /// Returns one routing decision per source channel in source stream order.
    #[must_use]
    pub const fn routes(&self) -> &[AudioChannelRoute] {
        &self.routes
    }
}

/// Sample-clock, channel, and routing semantics for an audio track.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioTrackSemantics {
    sample_rate: u32,
    channel_layout: ChannelLayout,
    routing: AudioRouting,
}

impl AudioTrackSemantics {
    /// Creates audio semantics with complete source-channel routing.
    pub fn new(
        sample_rate: u32,
        channel_layout: ChannelLayout,
        routing: AudioRouting,
    ) -> Result<Self> {
        Timebase::integer(sample_rate)?;
        let source_positions = channel_layout.positions();
        if routing.routes.len() != source_positions.len()
            || routing
                .routes
                .iter()
                .zip(source_positions)
                .any(|(route, position)| route.source != *position)
        {
            return Err(invalid_model(
                "create_audio_track_semantics",
                "audio routing must describe every source channel exactly once in stream order",
            ));
        }

        Ok(Self {
            sample_rate,
            channel_layout,
            routing,
        })
    }

    /// Returns the integral sample rate.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns the audio sample clock as a general timebase.
    #[must_use]
    pub fn timebase(&self) -> Timebase {
        Timebase::integer(self.sample_rate)
            .expect("AudioTrackSemantics validates a nonzero sample rate")
    }

    /// Returns the ordered source channel meanings.
    #[must_use]
    pub const fn channel_layout(&self) -> &ChannelLayout {
        &self.channel_layout
    }

    /// Returns the complete routing intent.
    #[must_use]
    pub const fn routing(&self) -> &AudioRouting {
        &self.routing
    }

    /// Returns a copy with checked replacement routing.
    pub fn with_routing(&self, routing: AudioRouting) -> Result<Self> {
        Self::new(self.sample_rate, self.channel_layout.clone(), routing)
    }

    /// Audits every adjacent audio span in caller-supplied record order.
    ///
    /// The report distinguishes silent record gaps, explicit overlap, source
    /// discontinuities within one clip, and changes to another linked clip.
    pub fn audit_continuity(&self, spans: &[AudioSpan]) -> Result<AudioContinuityReport> {
        for span in spans {
            if span.sample_rate() != self.sample_rate {
                return Err(invalid_model(
                    "audit_audio_continuity",
                    "audio span sample rate must match its track sample rate",
                ));
            }
        }

        let mut seams = Vec::with_capacity(spans.len().saturating_sub(1));
        for pair in spans.windows(2) {
            let left = &pair[0];
            let right = &pair[1];
            if right.record_start.sample() < left.record_start.sample() {
                return Err(invalid_model(
                    "audit_audio_continuity",
                    "audio spans must be supplied in nondecreasing record order",
                ));
            }

            let left_record_end = left.record_end()?;
            let record_delta = right
                .record_start
                .sample()
                .checked_sub(left_record_end.sample())
                .ok_or_else(|| {
                    invalid_model(
                        "audit_audio_continuity",
                        "audio seam exceeds the supported sample coordinate range",
                    )
                })?;
            let record = match record_delta.cmp(&0) {
                std::cmp::Ordering::Equal => AudioRecordContinuity::Seamless,
                std::cmp::Ordering::Greater => AudioRecordContinuity::Gap {
                    sample_count: record_delta.unsigned_abs(),
                },
                std::cmp::Ordering::Less => AudioRecordContinuity::Overlap {
                    sample_count: record_delta.unsigned_abs(),
                },
            };

            let source = if left.clip_id == right.clip_id {
                let expected = left.source_end()?;
                let actual = right.source_start;
                if expected == actual {
                    AudioSourceContinuity::Continuous
                } else {
                    AudioSourceContinuity::Discontinuous { expected, actual }
                }
            } else {
                AudioSourceContinuity::DifferentClip {
                    left: left.clip_id,
                    right: right.clip_id,
                }
            };

            seams.push(AudioSeam {
                left_clip_id: left.clip_id,
                right_clip_id: right.clip_id,
                record,
                source,
            });
        }

        Ok(AudioContinuityReport {
            seams: seams.into_boxed_slice(),
        })
    }
}

/// A sample-exact record placement linked to one editorial clip.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioSpan {
    clip_id: ClipId,
    record_start: SampleTime,
    source_start: SampleTime,
    sample_count: u64,
}

impl AudioSpan {
    /// Creates a linked audio placement on an exact sample boundary.
    pub fn new(
        clip_id: ClipId,
        record_start: RationalTime,
        source_start: SampleTime,
        sample_count: u64,
    ) -> Result<Self> {
        let record_start = record_start
            .checked_rescale(source_start.timebase(), TimeRounding::Exact)
            .map_err(|_| {
                invalid_model(
                    "create_audio_span",
                    "audio record start must fall on an exact sample boundary",
                )
            })?;
        let record_start = SampleTime::new(record_start.value(), source_start.sample_rate())?;
        Self::from_sample_starts(clip_id, record_start, source_start, sample_count)
    }

    fn from_sample_starts(
        clip_id: ClipId,
        record_start: SampleTime,
        source_start: SampleTime,
        sample_count: u64,
    ) -> Result<Self> {
        if record_start.sample_rate() != source_start.sample_rate() {
            return Err(invalid_model(
                "create_audio_span",
                "audio record and source positions must use one sample rate",
            ));
        }
        if sample_count == 0 {
            return Err(invalid_model(
                "create_audio_span",
                "audio span must contain at least one sample frame",
            ));
        }
        checked_sample_end(record_start, sample_count, "create_audio_span")?;
        checked_sample_end(source_start, sample_count, "create_audio_span")?;

        Ok(Self {
            clip_id,
            record_start,
            source_start,
            sample_count,
        })
    }

    /// Returns the linked editorial clip.
    #[must_use]
    pub const fn clip_id(&self) -> ClipId {
        self.clip_id
    }

    /// Returns the sample-exact record start.
    #[must_use]
    pub const fn record_start(&self) -> SampleTime {
        self.record_start
    }

    /// Returns the sample-exact source start.
    #[must_use]
    pub const fn source_start(&self) -> SampleTime {
        self.source_start
    }

    /// Returns the number of inter-channel sample frames.
    #[must_use]
    pub const fn sample_count(&self) -> u64 {
        self.sample_count
    }

    /// Returns the integral sample rate shared by record and source positions.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.source_start.sample_rate()
    }

    /// Returns the exclusive record end.
    pub fn record_end(&self) -> Result<SampleTime> {
        checked_sample_end(
            self.record_start,
            self.sample_count,
            "audio_span_record_end",
        )
    }

    /// Returns the exclusive source end.
    pub fn source_end(&self) -> Result<SampleTime> {
        checked_sample_end(
            self.source_start,
            self.sample_count,
            "audio_span_source_end",
        )
    }

    /// Returns the exact half-open record range on the sample clock.
    pub fn record_range(&self) -> Result<TimeRange> {
        TimeRange::new(
            self.record_start.rational_time(),
            Duration::from_samples(self.sample_count, self.sample_rate())?,
        )
    }

    /// Returns the exact half-open source range on the sample clock.
    pub fn source_range(&self) -> Result<TimeRange> {
        TimeRange::new(
            self.source_start.rational_time(),
            Duration::from_samples(self.sample_count, self.sample_rate())?,
        )
    }

    /// Splits the placement while preserving the linked clip and exact mapping.
    pub fn split_at(&self, sample_offset: u64) -> Result<(Self, Self)> {
        if sample_offset == 0 || sample_offset >= self.sample_count {
            return Err(invalid_model(
                "split_audio_span",
                "audio split must fall strictly inside the span",
            ));
        }
        let left = Self::from_sample_starts(
            self.clip_id,
            self.record_start,
            self.source_start,
            sample_offset,
        )?;
        let right = Self::from_sample_starts(
            self.clip_id,
            checked_sample_end(self.record_start, sample_offset, "split_audio_span")?,
            checked_sample_end(self.source_start, sample_offset, "split_audio_span")?,
            self.sample_count - sample_offset,
        )?;
        Ok((left, right))
    }

    /// Removes sample frames from the start while preserving record-to-source sync.
    pub fn trim_start(&self, sample_count: u64) -> Result<Self> {
        if sample_count >= self.sample_count {
            return Err(invalid_model(
                "trim_audio_span_start",
                "audio trim must leave at least one sample frame",
            ));
        }
        Self::from_sample_starts(
            self.clip_id,
            checked_sample_end(self.record_start, sample_count, "trim_audio_span_start")?,
            checked_sample_end(self.source_start, sample_count, "trim_audio_span_start")?,
            self.sample_count - sample_count,
        )
    }

    /// Removes sample frames from the end without changing either start mapping.
    pub fn trim_end(&self, sample_count: u64) -> Result<Self> {
        if sample_count >= self.sample_count {
            return Err(invalid_model(
                "trim_audio_span_end",
                "audio trim must leave at least one sample frame",
            ));
        }
        Self::from_sample_starts(
            self.clip_id,
            self.record_start,
            self.source_start,
            self.sample_count - sample_count,
        )
    }
}

/// Record coverage at one adjacent audio seam.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum AudioRecordContinuity {
    /// The right span begins exactly where the left span ends.
    Seamless,
    /// No span covers this many sample frames.
    Gap {
        /// Exact gap size on the track sample clock.
        sample_count: u64,
    },
    /// Both spans cover this many sample frames.
    Overlap {
        /// Exact overlap size on the track sample clock.
        sample_count: u64,
    },
}

/// Source relationship at one adjacent audio seam.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum AudioSourceContinuity {
    /// One linked clip continues at its next source sample.
    Continuous,
    /// One linked clip jumps or repeats source samples.
    Discontinuous {
        /// The next sample implied by the left span.
        expected: SampleTime,
        /// The actual first sample of the right span.
        actual: SampleTime,
    },
    /// The seam moves from one linked clip to another.
    DifferentClip {
        /// Clip linked by the left span.
        left: ClipId,
        /// Clip linked by the right span.
        right: ClipId,
    },
}

/// Complete structural continuity at one adjacent audio seam.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioSeam {
    left_clip_id: ClipId,
    right_clip_id: ClipId,
    record: AudioRecordContinuity,
    source: AudioSourceContinuity,
}

impl AudioSeam {
    /// Returns the linked clip before the seam.
    #[must_use]
    pub const fn left_clip_id(&self) -> ClipId {
        self.left_clip_id
    }

    /// Returns the linked clip after the seam.
    #[must_use]
    pub const fn right_clip_id(&self) -> ClipId {
        self.right_clip_id
    }

    /// Returns record coverage at the seam.
    #[must_use]
    pub const fn record(&self) -> AudioRecordContinuity {
        self.record
    }

    /// Returns the source relationship at the seam.
    #[must_use]
    pub const fn source(&self) -> AudioSourceContinuity {
        self.source
    }
}

/// An ordered audit of every adjacent seam on an audio track.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioContinuityReport {
    seams: Box<[AudioSeam]>,
}

impl AudioContinuityReport {
    /// Returns seams in the caller-supplied record order.
    #[must_use]
    pub const fn seams(&self) -> &[AudioSeam] {
        &self.seams
    }

    /// Returns true when record coverage has no structurally silent gap.
    ///
    /// This reports timeline coverage, not whether waveform samples are silent.
    #[must_use]
    pub fn has_uninterrupted_record_coverage(&self) -> bool {
        self.seams
            .iter()
            .all(|seam| !matches!(seam.record, AudioRecordContinuity::Gap { .. }))
    }
}

/// A normalized, syntactically bounded BCP 47 language tag.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct LanguageTag(String);

impl LanguageTag {
    /// Validates ASCII subtag syntax and normalizes semantically irrelevant case.
    ///
    /// This validates stable storage syntax but does not claim that every subtag
    /// is currently registered by IANA.
    pub fn new(tag: impl Into<String>) -> Result<Self> {
        let tag = tag.into().to_ascii_lowercase();
        if tag.is_empty() || tag.len() > 255 || !tag.is_ascii() {
            return Err(invalid_model(
                "create_language_tag",
                "language tag must contain between 1 and 255 ASCII bytes",
            ));
        }
        if !is_well_formed_language_tag(&tag) {
            return Err(invalid_model(
                "create_language_tag",
                "language tag contains invalid BCP 47 syntax",
            ));
        }
        Ok(Self(tag))
    }

    /// Returns the normalized language tag.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

fn is_well_formed_language_tag(tag: &str) -> bool {
    const GRANDFATHERED: &[&str] = &[
        "art-lojban",
        "cel-gaulish",
        "en-gb-oed",
        "i-ami",
        "i-bnn",
        "i-default",
        "i-enochian",
        "i-hak",
        "i-klingon",
        "i-lux",
        "i-mingo",
        "i-navajo",
        "i-pwn",
        "i-tao",
        "i-tay",
        "i-tsu",
        "no-bok",
        "no-nyn",
        "sgn-be-fr",
        "sgn-be-nl",
        "sgn-ch-de",
        "zh-guoyu",
        "zh-hakka",
        "zh-min",
        "zh-min-nan",
        "zh-xiang",
    ];

    if GRANDFATHERED.contains(&tag) {
        return true;
    }

    let subtags: Vec<_> = tag.split('-').collect();
    if subtags.iter().any(|subtag| {
        subtag.is_empty()
            || subtag.len() > 8
            || !subtag.bytes().all(|byte| byte.is_ascii_alphanumeric())
    }) {
        return false;
    }

    if subtags[0] == "x" {
        return subtags.len() > 1;
    }

    let language = subtags[0];
    if !(2..=8).contains(&language.len())
        || !language.bytes().all(|byte| byte.is_ascii_alphabetic())
    {
        return false;
    }

    let mut index = 1;
    if language.len() <= 3 {
        for _ in 0..3 {
            if index < subtags.len()
                && subtags[index].len() == 3
                && subtags[index]
                    .bytes()
                    .all(|byte| byte.is_ascii_alphabetic())
            {
                index += 1;
            } else {
                break;
            }
        }
    }

    if index < subtags.len()
        && subtags[index].len() == 4
        && subtags[index]
            .bytes()
            .all(|byte| byte.is_ascii_alphabetic())
    {
        index += 1;
    }
    if index < subtags.len()
        && ((subtags[index].len() == 2
            && subtags[index]
                .bytes()
                .all(|byte| byte.is_ascii_alphabetic()))
            || (subtags[index].len() == 3
                && subtags[index].bytes().all(|byte| byte.is_ascii_digit())))
    {
        index += 1;
    }

    let mut variants = Vec::new();
    while index < subtags.len() && is_language_variant(subtags[index]) {
        if variants.contains(&subtags[index]) {
            return false;
        }
        variants.push(subtags[index]);
        index += 1;
    }

    let mut singletons = Vec::new();
    while index < subtags.len() && subtags[index].len() == 1 && subtags[index] != "x" {
        let singleton = subtags[index];
        if singletons.contains(&singleton) {
            return false;
        }
        singletons.push(singleton);
        index += 1;

        let extension_start = index;
        while index < subtags.len() && subtags[index].len() >= 2 {
            index += 1;
        }
        if index == extension_start {
            return false;
        }
    }

    if index < subtags.len() && subtags[index] == "x" {
        index += 1;
        if index == subtags.len() {
            return false;
        }
        index = subtags.len();
    }

    index == subtags.len()
}

fn is_language_variant(subtag: &str) -> bool {
    (subtag.len() >= 5 && subtag.bytes().all(|byte| byte.is_ascii_alphanumeric()))
        || (subtag.len() == 4
            && subtag.as_bytes()[0].is_ascii_digit()
            && subtag[1..].bytes().all(|byte| byte.is_ascii_alphanumeric()))
}

/// Intended presentation of a timed-text track.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum CaptionPurpose {
    /// Same-language dialogue and meaningful sound transcription.
    Captions,
    /// Dialogue translated or transcribed for another language context.
    Subtitles,
    /// Textual descriptions of visual content.
    Descriptions,
    /// Navigable chapter titles.
    Chapters,
}

/// Exact timing, language, and presentation semantics for timed text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptionTrackSemantics {
    timebase: Timebase,
    language: LanguageTag,
    purpose: CaptionPurpose,
}

impl CaptionTrackSemantics {
    /// Creates explicit timed-text semantics.
    #[must_use]
    pub const fn new(timebase: Timebase, language: LanguageTag, purpose: CaptionPurpose) -> Self {
        Self {
            timebase,
            language,
            purpose,
        }
    }

    /// Returns the exact cue edit clock.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.timebase
    }

    /// Returns the normalized BCP 47 language tag.
    #[must_use]
    pub const fn language(&self) -> &LanguageTag {
        &self.language
    }

    /// Returns the intended timed-text presentation.
    #[must_use]
    pub const fn purpose(&self) -> CaptionPurpose {
        self.purpose
    }
}

/// The application-defined type identity of timed data events.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DataSchema {
    scheme_id_uri: String,
    value: Option<String>,
}

impl DataSchema {
    /// Creates a bounded scheme identifier and optional discriminator.
    pub fn new(scheme_id_uri: impl Into<String>, value: Option<&str>) -> Result<Self> {
        let scheme_id_uri = scheme_id_uri.into();
        validate_data_label(
            &scheme_id_uri,
            "create_data_schema",
            "data scheme identifier must be nonempty bounded ASCII without whitespace",
        )?;
        let value = value.map(str::to_owned);
        if let Some(value) = &value {
            validate_data_label(
                value,
                "create_data_schema",
                "data schema value must be nonempty bounded ASCII without control characters",
            )?;
        }
        Ok(Self {
            scheme_id_uri,
            value,
        })
    }

    /// Returns the stable scheme identifier URI.
    #[must_use]
    pub fn scheme_id_uri(&self) -> &str {
        &self.scheme_id_uri
    }

    /// Returns the optional scheme-specific discriminator.
    #[must_use]
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }
}

/// Exact timing and payload type semantics for timed data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DataTrackSemantics {
    timebase: Timebase,
    schema: DataSchema,
}

impl DataTrackSemantics {
    /// Creates explicit timed-data semantics.
    #[must_use]
    pub const fn new(timebase: Timebase, schema: DataSchema) -> Self {
        Self { timebase, schema }
    }

    /// Returns the exact event edit clock.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.timebase
    }

    /// Returns the application-defined payload type identity.
    #[must_use]
    pub const fn schema(&self) -> &DataSchema {
        &self.schema
    }
}

fn checked_sample_end(
    start: SampleTime,
    sample_count: u64,
    operation: &'static str,
) -> Result<SampleTime> {
    let sample_count = i64::try_from(sample_count).map_err(|_| {
        invalid_model(
            operation,
            "audio span exceeds the supported sample coordinate range",
        )
    })?;
    let sample = start.sample().checked_add(sample_count).ok_or_else(|| {
        invalid_model(
            operation,
            "audio span exceeds the supported sample coordinate range",
        )
    })?;
    SampleTime::new(sample, start.sample_rate())
}

fn validate_data_label(value: &str, operation: &'static str, message: &'static str) -> Result<()> {
    if value.is_empty()
        || value.len() > 1_024
        || !value.is_ascii()
        || value
            .bytes()
            .any(|byte| byte.is_ascii_whitespace() || byte.is_ascii_control())
    {
        return Err(invalid_model(operation, message));
    }
    Ok(())
}

fn invalid_model(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-timeline.model", operation))
}

/// The stable identity of any item that can appear in an editorial track.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum EditorialObjectId {
    /// A source-bearing clip.
    Clip(ClipId),
    /// An explicit empty interval.
    Gap(GapId),
    /// A transition between adjacent items.
    Transition(TransitionId),
    /// A generated-media item.
    Generator(GeneratorId),
    /// A timed caption item.
    Caption(CaptionId),
}

impl std::fmt::Display for EditorialObjectId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Clip(id) => id.fmt(formatter),
            Self::Gap(id) => id.fmt(formatter),
            Self::Transition(id) => id.fmt(formatter),
            Self::Generator(id) => id.fmt(formatter),
            Self::Caption(id) => id.fmt(formatter),
        }
    }
}

/// The source relationship retained by a clip.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ClipSource {
    /// Source media linked by the project.
    Media(MediaId),
    /// A nested editable timeline in the same project.
    Timeline(TimelineId),
}

/// One exact, synchronized mapping between source and record coordinates.
///
/// Source and record ranges retain their own clocks. Their physical durations
/// must be equal, and coordinate conversion never rounds implicitly.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClipRangeMap {
    source_range: TimeRange,
    record_range: TimeRange,
}

impl ClipRangeMap {
    /// Creates a nonempty synchronized source-to-record mapping.
    pub fn new(source_range: TimeRange, record_range: TimeRange) -> Result<Self> {
        if source_range.is_empty() || record_range.is_empty() {
            return Err(invalid(
                "create_clip_range_map",
                "clip source and record ranges must both be nonempty",
            ));
        }
        if source_range.duration().rational_time() != record_range.duration().rational_time() {
            return Err(invalid(
                "create_clip_range_map",
                "clip source and record durations must represent equal rational time",
            ));
        }
        Ok(Self {
            source_range,
            record_range,
        })
    }

    /// Returns the selected interval in source coordinates.
    #[must_use]
    pub const fn source_range(self) -> TimeRange {
        self.source_range
    }

    /// Returns the placement interval in record coordinates.
    #[must_use]
    pub const fn record_range(self) -> TimeRange {
        self.record_range
    }

    /// Maps one source coordinate to the record clock without rounding.
    pub fn source_time_to_record(self, time: RationalTime) -> Result<RationalTime> {
        map_clip_time(
            self.source_range,
            self.record_range,
            time,
            "map_clip_source_time",
        )
    }

    /// Maps one record coordinate to the source clock without rounding.
    pub fn record_time_to_source(self, time: RationalTime) -> Result<RationalTime> {
        map_clip_time(
            self.record_range,
            self.source_range,
            time,
            "map_clip_record_time",
        )
    }

    /// Maps a source subrange to record coordinates without rounding.
    pub fn source_range_to_record(self, range: TimeRange) -> Result<TimeRange> {
        map_clip_subrange(
            self.source_range,
            self.record_range,
            range,
            "map_clip_source_range",
        )
    }

    /// Maps a record subrange to source coordinates without rounding.
    pub fn record_range_to_source(self, range: TimeRange) -> Result<TimeRange> {
        map_clip_subrange(
            self.record_range,
            self.source_range,
            range,
            "map_clip_record_range",
        )
    }
}

fn map_clip_time(
    from: TimeRange,
    to: TimeRange,
    time: RationalTime,
    operation: &'static str,
) -> Result<RationalTime> {
    if !from.contains(time)? {
        return Err(invalid(
            operation,
            "mapped time must lie inside the half-open clip range",
        ));
    }
    map_clip_endpoint(from, to, time)
}

fn map_clip_subrange(
    from: TimeRange,
    to: TimeRange,
    range: TimeRange,
    operation: &'static str,
) -> Result<TimeRange> {
    let range_end = range.end_exclusive()?;
    if range.start() < from.start() || range_end > from.end_exclusive()? {
        return Err(invalid(
            operation,
            "mapped range must lie inside the clip range",
        ));
    }
    let start = map_clip_endpoint(from, to, range.start())?;
    let end = map_clip_endpoint(from, to, range_end)?;
    TimeRange::from_start_end(start, end)
}

fn map_clip_endpoint(from: TimeRange, to: TimeRange, time: RationalTime) -> Result<RationalTime> {
    let offset = time.checked_sub_at(from.start(), from.timebase(), TimeRounding::Exact)?;
    let offset = offset.checked_rescale(to.timebase(), TimeRounding::Exact)?;
    to.start()
        .checked_add_at(offset, to.timebase(), TimeRounding::Exact)
}

/// How a selected source interval relates to known source availability.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RangeAvailability {
    /// The source extent has not been discovered or supplied.
    Unknown,
    /// Every selected source coordinate is currently available.
    FullyAvailable,
    /// Some, but not all, selected source coordinates are currently available.
    PartiallyAvailable,
    /// No selected source coordinate is currently available.
    Unavailable,
}

/// Resolved range meaning for one clip and its linked source.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ClipRangeContext {
    source: ClipSource,
    ranges: ClipRangeMap,
    time_map: ClipTimeMap,
    available_range: Option<TimeRange>,
}

impl ClipRangeContext {
    /// Returns the linked media or nested timeline identity.
    #[must_use]
    pub const fn source(&self) -> ClipSource {
        self.source
    }

    /// Returns the synchronized source-to-record mapping.
    #[must_use]
    pub const fn ranges(&self) -> ClipRangeMap {
        self.ranges
    }

    /// Returns the selected interval in source coordinates.
    #[must_use]
    pub const fn source_range(&self) -> TimeRange {
        self.ranges.source_range()
    }

    /// Returns the placement interval in record coordinates.
    #[must_use]
    pub const fn record_range(&self) -> TimeRange {
        self.ranges.record_range()
    }

    /// Returns the complete clip-local record-to-source timing map.
    #[must_use]
    pub const fn time_map(&self) -> &ClipTimeMap {
        &self.time_map
    }

    /// Returns known media or nested-timeline availability.
    #[must_use]
    pub const fn available_range(&self) -> Option<TimeRange> {
        self.available_range
    }

    /// Classifies the selected source interval without changing either range.
    pub fn availability(&self) -> Result<RangeAvailability> {
        let Some(available) = self.available_range else {
            return Ok(RangeAvailability::Unknown);
        };
        let source = self.source_range();
        if available.start() <= source.start()
            && available.end_exclusive()? >= source.end_exclusive()?
        {
            return Ok(RangeAvailability::FullyAvailable);
        }
        if available.intersects(source)? {
            return Ok(RangeAvailability::PartiallyAvailable);
        }
        Ok(RangeAvailability::Unavailable)
    }

    /// Resolves one absolute record coordinate for immediate transport use.
    pub fn playback_sample(
        &self,
        record_time: RationalTime,
        rounding: TimeRounding,
    ) -> Result<ClipPlaybackSample> {
        if !self.record_range().contains(record_time)? {
            return Err(invalid(
                "resolve_clip_playback_sample",
                "playback query must lie inside the half-open clip record range",
            ));
        }
        let local_time = record_time.checked_sub_at(
            self.record_range().start(),
            self.record_range().timebase(),
            TimeRounding::Exact,
        )?;
        let mapped = self.time_map.source_time_at(local_time, rounding)?;
        let availability = match self.available_range {
            None => SampleAvailability::Unknown,
            Some(available) if available.contains(mapped.time())? => SampleAvailability::Available,
            Some(_) => SampleAvailability::Unavailable,
        };
        Ok(ClipPlaybackSample {
            mapped,
            availability,
        })
    }
}

/// Known source availability for one resolved playback sample.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SampleAvailability {
    /// Source extent discovery has not supplied an answer.
    Unknown,
    /// The selected source coordinate is available.
    Available,
    /// The selected source coordinate lies outside known availability.
    Unavailable,
}

/// One transport-ready source coordinate with visible degraded behavior.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ClipPlaybackSample {
    mapped: MappedSourceTime,
    availability: SampleAvailability,
}

impl ClipPlaybackSample {
    /// Returns the selected source coordinate.
    #[must_use]
    pub const fn source_time(self) -> RationalTime {
        self.mapped.time()
    }

    /// Returns exact, held, or explicitly rounded resolution.
    #[must_use]
    pub const fn resolution(self) -> RetimeResolution {
        self.mapped.resolution()
    }

    /// Returns the directly inspectable time-map segment index.
    #[must_use]
    pub const fn segment_index(self) -> usize {
        self.mapped.segment_index()
    }

    /// Returns known, unknown, or unavailable source state.
    #[must_use]
    pub const fn availability(self) -> SampleAvailability {
        self.availability
    }
}

/// A source-bearing clip instance with explicit source and record mappings.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Clip {
    id: ClipId,
    name: String,
    source: ClipSource,
    ranges: ClipRangeMap,
    time_map: ClipTimeMap,
}

impl Clip {
    /// Creates a clip instance.
    pub fn new(
        id: ClipId,
        name: impl Into<String>,
        source: ClipSource,
        source_range: TimeRange,
        record_range: TimeRange,
    ) -> Result<Self> {
        let ranges = ClipRangeMap::new(source_range, record_range)?;
        let time_map = ClipTimeMap::identity(record_range.duration(), source_range.start())?;
        Ok(Self {
            id,
            name: name.into(),
            source,
            ranges,
            time_map,
        })
    }

    /// Returns the clip identity.
    #[must_use]
    pub const fn id(&self) -> ClipId {
        self.id
    }

    /// Returns the editor-facing name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the media or nested-timeline relationship.
    #[must_use]
    pub const fn source(&self) -> ClipSource {
        self.source
    }

    /// Returns the selected interval in source coordinates.
    #[must_use]
    pub const fn source_range(&self) -> TimeRange {
        self.ranges.source_range()
    }

    /// Returns the placement interval in timeline coordinates.
    #[must_use]
    pub const fn record_range(&self) -> TimeRange {
        self.ranges.record_range()
    }

    /// Returns the complete synchronized source-to-record mapping.
    #[must_use]
    pub const fn ranges(&self) -> ClipRangeMap {
        self.ranges
    }

    /// Returns the complete clip-local record-to-source timing map.
    #[must_use]
    pub const fn time_map(&self) -> &ClipTimeMap {
        &self.time_map
    }

    /// Resolves one absolute record coordinate through this clip's time map.
    pub fn source_time_at(
        &self,
        record_time: RationalTime,
        rounding: TimeRounding,
    ) -> Result<MappedSourceTime> {
        if !self.record_range().contains(record_time)? {
            return Err(invalid(
                "resolve_clip_source_time",
                "playback query must lie inside the half-open clip record range",
            ));
        }
        let local_time = record_time.checked_sub_at(
            self.record_range().start(),
            self.record_range().timebase(),
            TimeRounding::Exact,
        )?;
        self.time_map.source_time_at(local_time, rounding)
    }

    /// Replaces the source relationship inside an unpublished draft.
    pub fn set_source(&mut self, source: ClipSource) {
        self.source = source;
    }

    /// Replaces the editor-facing name inside an unpublished draft.
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    /// Replaces the selected source interval inside an unpublished draft.
    pub fn set_source_range(&mut self, source_range: TimeRange) -> Result<()> {
        let ranges = ClipRangeMap::new(source_range, self.record_range())?;
        let time_map = self
            .time_map
            .translate_source(self.source_range().start(), source_range.start())?;
        self.ranges = ranges;
        self.time_map = time_map;
        Ok(())
    }

    /// Replaces the timeline placement inside an unpublished draft.
    pub fn set_record_range(&mut self, record_range: TimeRange) -> Result<()> {
        let ranges = ClipRangeMap::new(self.source_range(), record_range)?;
        let time_map = self
            .time_map
            .rescale_record_clock(record_range.duration())?;
        self.ranges = ranges;
        self.time_map = time_map;
        Ok(())
    }

    /// Replaces source selection and record placement as one checked operation.
    pub fn set_ranges(&mut self, source_range: TimeRange, record_range: TimeRange) -> Result<()> {
        let ranges = ClipRangeMap::new(source_range, record_range)?;
        let time_map = if self.record_range().duration() == record_range.duration() {
            self.time_map
                .rescale_record_clock(record_range.duration())?
                .translate_source(self.source_range().start(), source_range.start())?
        } else if self.time_map.mode() == RetimeMode::Identity {
            ClipTimeMap::identity(record_range.duration(), source_range.start())?
        } else {
            return Err(invalid(
                "replace_clip_ranges",
                "retimed range replacement must preserve the clip duration or supply a new time map",
            ));
        };
        self.ranges = ranges;
        self.time_map = time_map;
        Ok(())
    }

    /// Replaces the complete playback timing as one checked operation.
    pub fn set_time_map(&mut self, time_map: ClipTimeMap) -> Result<()> {
        time_map.validate_binding(
            self.record_range().duration(),
            self.source_range().timebase(),
        )?;
        self.time_map = time_map;
        Ok(())
    }

    pub(crate) fn clone_with_id(&self, id: ClipId) -> Self {
        let mut cloned = self.clone();
        cloned.id = id;
        cloned
    }

    pub(crate) fn slice_with_id(&self, id: ClipId, record_range: TimeRange) -> Result<Self> {
        let source_range = self.ranges.record_range_to_source(record_range)?;
        let local_start = record_range.start().checked_sub_at(
            self.record_range().start(),
            self.record_range().timebase(),
            TimeRounding::Exact,
        )?;
        let local_range = TimeRange::new(local_start, record_range.duration())?;
        let time_map = self.time_map.slice(local_range)?;
        Ok(Self {
            id,
            name: self.name.clone(),
            source: self.source,
            ranges: ClipRangeMap::new(source_range, record_range)?,
            time_map,
        })
    }
}

/// An explicit empty interval on a track.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Gap {
    id: GapId,
    name: String,
    record_range: TimeRange,
}

impl Gap {
    /// Creates an explicit track gap.
    pub fn new(id: GapId, name: impl Into<String>, record_range: TimeRange) -> Self {
        Self {
            id,
            name: name.into(),
            record_range,
        }
    }

    /// Returns the gap identity.
    #[must_use]
    pub const fn id(&self) -> GapId {
        self.id
    }

    /// Returns the editor-facing name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the empty interval in timeline coordinates.
    #[must_use]
    pub const fn record_range(&self) -> TimeRange {
        self.record_range
    }

    /// Replaces the gap placement inside an unpublished draft.
    pub fn set_record_range(&mut self, record_range: TimeRange) {
        self.record_range = record_range;
    }

    /// Replaces the editor-facing name inside an unpublished draft.
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }
}

/// An editable generated-media item.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Generator {
    id: GeneratorId,
    name: String,
    kind: String,
    parameters: BTreeMap<String, String>,
    record_range: TimeRange,
}

impl Generator {
    /// Creates a generated-media item with deterministically ordered parameters.
    pub fn new(
        id: GeneratorId,
        name: impl Into<String>,
        kind: impl Into<String>,
        parameters: BTreeMap<String, String>,
        record_range: TimeRange,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            kind: kind.into(),
            parameters,
            record_range,
        }
    }

    /// Returns the generator identity.
    #[must_use]
    pub const fn id(&self) -> GeneratorId {
        self.id
    }

    /// Returns the editor-facing name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the stable generator kind.
    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Returns parameters in deterministic key order.
    #[must_use]
    pub const fn parameters(&self) -> &BTreeMap<String, String> {
        &self.parameters
    }

    /// Returns the generator placement in timeline coordinates.
    #[must_use]
    pub const fn record_range(&self) -> TimeRange {
        self.record_range
    }

    /// Replaces one parameter inside an unpublished draft.
    pub fn set_parameter(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.parameters.insert(key.into(), value.into());
    }

    /// Replaces the editor-facing name inside an unpublished draft.
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    /// Replaces the stable generator kind inside an unpublished draft.
    pub fn set_kind(&mut self, kind: impl Into<String>) {
        self.kind = kind.into();
    }

    /// Replaces the generator placement inside an unpublished draft.
    pub fn set_record_range(&mut self, record_range: TimeRange) {
        self.record_range = record_range;
    }
}

/// An editable timed caption.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Caption {
    id: CaptionId,
    name: String,
    text: String,
    language: Option<String>,
    record_range: TimeRange,
}

impl Caption {
    /// Creates a caption item.
    pub fn new(
        id: CaptionId,
        name: impl Into<String>,
        text: impl Into<String>,
        language: Option<String>,
        record_range: TimeRange,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            text: text.into(),
            language,
            record_range,
        }
    }

    /// Returns the caption identity.
    #[must_use]
    pub const fn id(&self) -> CaptionId {
        self.id
    }

    /// Returns the editor-facing name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the caption text.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns the optional language tag.
    #[must_use]
    pub fn language(&self) -> Option<&str> {
        self.language.as_deref()
    }

    /// Returns the caption placement in timeline coordinates.
    #[must_use]
    pub const fn record_range(&self) -> TimeRange {
        self.record_range
    }

    /// Replaces the caption text inside an unpublished draft.
    pub fn set_text(&mut self, text: impl Into<String>) {
        self.text = text.into();
    }

    /// Replaces the editor-facing name inside an unpublished draft.
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    /// Replaces the optional language tag inside an unpublished draft.
    pub fn set_language(&mut self, language: Option<String>) {
        self.language = language;
    }

    /// Replaces the caption placement inside an unpublished draft.
    pub fn set_record_range(&mut self, record_range: TimeRange) {
        self.record_range = record_range;
    }
}

/// A transition between two adjacent non-transition track items.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Transition {
    id: TransitionId,
    name: String,
    from: EditorialObjectId,
    to: EditorialObjectId,
    from_offset: Duration,
    to_offset: Duration,
}

impl Transition {
    /// Creates a transition relationship.
    pub fn new(
        id: TransitionId,
        name: impl Into<String>,
        from: EditorialObjectId,
        to: EditorialObjectId,
        from_offset: Duration,
        to_offset: Duration,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            from,
            to,
            from_offset,
            to_offset,
        }
    }

    /// Returns the transition identity.
    #[must_use]
    pub const fn id(&self) -> TransitionId {
        self.id
    }

    /// Returns the editor-facing name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the preceding endpoint.
    #[must_use]
    pub const fn from(&self) -> EditorialObjectId {
        self.from
    }

    /// Returns the following endpoint.
    #[must_use]
    pub const fn to(&self) -> EditorialObjectId {
        self.to
    }

    /// Returns the amount consumed from the preceding item.
    #[must_use]
    pub const fn from_offset(&self) -> Duration {
        self.from_offset
    }

    /// Returns the amount consumed from the following item.
    #[must_use]
    pub const fn to_offset(&self) -> Duration {
        self.to_offset
    }

    /// Replaces the editor-facing name inside an unpublished draft.
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    /// Replaces both adjacency endpoints inside an unpublished draft.
    pub fn set_endpoints(&mut self, from: EditorialObjectId, to: EditorialObjectId) {
        self.from = from;
        self.to = to;
    }

    /// Replaces both overlap offsets inside an unpublished draft.
    pub fn set_offsets(&mut self, from_offset: Duration, to_offset: Duration) {
        self.from_offset = from_offset;
        self.to_offset = to_offset;
    }
}

/// One ordered item in an editorial track.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TrackItem {
    /// A source-bearing clip.
    Clip(Clip),
    /// An explicit gap.
    Gap(Gap),
    /// A transition between adjacent items.
    Transition(Transition),
    /// A generated-media item.
    Generator(Generator),
    /// A timed caption.
    Caption(Caption),
}

impl TrackItem {
    /// Returns the stable object identity.
    #[must_use]
    pub const fn id(&self) -> EditorialObjectId {
        match self {
            Self::Clip(value) => EditorialObjectId::Clip(value.id()),
            Self::Gap(value) => EditorialObjectId::Gap(value.id()),
            Self::Transition(value) => EditorialObjectId::Transition(value.id()),
            Self::Generator(value) => EditorialObjectId::Generator(value.id()),
            Self::Caption(value) => EditorialObjectId::Caption(value.id()),
        }
    }

    /// Returns the record range, or `None` for a transition.
    #[must_use]
    pub const fn record_range(&self) -> Option<TimeRange> {
        match self {
            Self::Clip(value) => Some(value.record_range()),
            Self::Gap(value) => Some(value.record_range()),
            Self::Transition(_) => None,
            Self::Generator(value) => Some(value.record_range()),
            Self::Caption(value) => Some(value.record_range()),
        }
    }

    /// Views this item as a clip.
    #[must_use]
    pub const fn as_clip(&self) -> Option<&Clip> {
        match self {
            Self::Clip(value) => Some(value),
            _ => None,
        }
    }

    /// Mutably views this item as a clip inside an unpublished draft.
    pub fn as_clip_mut(&mut self) -> Option<&mut Clip> {
        match self {
            Self::Clip(value) => Some(value),
            _ => None,
        }
    }

    /// Views this item as a caption.
    #[must_use]
    pub const fn as_caption(&self) -> Option<&Caption> {
        match self {
            Self::Caption(value) => Some(value),
            _ => None,
        }
    }

    /// Mutably views this item as a caption inside an unpublished draft.
    pub fn as_caption_mut(&mut self) -> Option<&mut Caption> {
        match self {
            Self::Caption(value) => Some(value),
            _ => None,
        }
    }
}

/// One ordered editorial track.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Track {
    id: TrackId,
    name: String,
    semantics: TrackSemantics,
    items: Vec<TrackItem>,
}

impl Track {
    /// Creates a track with its explicit item order.
    pub fn new(
        id: TrackId,
        name: impl Into<String>,
        semantics: TrackSemantics,
        items: Vec<TrackItem>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            semantics,
            items,
        }
    }

    /// Returns the track identity.
    #[must_use]
    pub const fn id(&self) -> TrackId {
        self.id
    }

    /// Returns the editor-facing track name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the broad media role.
    #[must_use]
    pub const fn kind(&self) -> TrackKind {
        self.semantics.kind()
    }

    /// Returns the complete editable track semantics.
    #[must_use]
    pub const fn semantics(&self) -> &TrackSemantics {
        &self.semantics
    }

    /// Returns all items in editorial order.
    #[must_use]
    pub fn items(&self) -> &[TrackItem] {
        &self.items
    }

    /// Looks up an item by its stable typed identity.
    #[must_use]
    pub fn item(&self, id: EditorialObjectId) -> Option<&TrackItem> {
        self.items.iter().find(|item| item.id() == id)
    }

    /// Mutably looks up an item inside an unpublished draft.
    pub fn item_mut(&mut self, id: EditorialObjectId) -> Result<&mut TrackItem> {
        self.items
            .iter_mut()
            .find(|item| item.id() == id)
            .ok_or_else(|| not_found("find_item", "editorial item was not found", "item", id))
    }

    /// Replaces the complete item order inside an unpublished draft.
    pub fn replace_items(&mut self, items: Vec<TrackItem>) {
        self.items = items;
    }

    /// Replaces the editor-facing track name inside an unpublished draft.
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    /// Replaces the complete track semantics inside an unpublished draft.
    pub fn set_semantics(&mut self, semantics: TrackSemantics) {
        self.semantics = semantics;
    }

    fn end_time(&self) -> Result<RationalTime> {
        self.items
            .iter()
            .rev()
            .find_map(TrackItem::record_range)
            .map(TimeRange::end_exclusive)
            .transpose()?
            .map_or_else(|| Ok(RationalTime::zero(self.semantics.timebase())), Ok)
    }
}

/// One editable sequence with an exact record clock.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Timeline {
    id: TimelineId,
    name: String,
    edit_rate: Timebase,
    global_start: RationalTime,
    tracks: Vec<Track>,
    edit_state: TimelineEditState,
    annotations: TimelineAnnotations,
    multicam_source: Option<MulticamSource>,
    multicam_clips: BTreeMap<ClipId, MulticamClip>,
}

impl Timeline {
    /// Creates a timeline.
    pub fn new(
        id: TimelineId,
        name: impl Into<String>,
        edit_rate: Timebase,
        global_start: RationalTime,
        tracks: Vec<Track>,
    ) -> Self {
        let edit_state = TimelineEditState::from_tracks(&tracks);
        Self {
            id,
            name: name.into(),
            edit_rate,
            global_start,
            tracks,
            edit_state,
            annotations: TimelineAnnotations::default(),
            multicam_source: None,
            multicam_clips: BTreeMap::new(),
        }
    }

    /// Returns the timeline identity.
    #[must_use]
    pub const fn id(&self) -> TimelineId {
        self.id
    }

    /// Returns the editor-facing timeline name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the exact record clock.
    #[must_use]
    pub const fn edit_rate(&self) -> Timebase {
        self.edit_rate
    }

    /// Returns the external start coordinate.
    #[must_use]
    pub const fn global_start(&self) -> RationalTime {
        self.global_start
    }

    /// Returns tracks in bottom-to-top project order.
    #[must_use]
    pub fn tracks(&self) -> &[Track] {
        &self.tracks
    }

    /// Returns authoritative selection, targeting, synchronization, and relationship intent.
    #[must_use]
    pub const fn edit_state(&self) -> &TimelineEditState {
        &self.edit_state
    }

    /// Returns synchronized source angle state when this is a multicam source timeline.
    #[must_use]
    pub const fn multicam_source(&self) -> Option<&MulticamSource> {
        self.multicam_source.as_ref()
    }

    /// Mutably returns synchronized source angle state inside an unpublished draft.
    pub fn multicam_source_mut(&mut self) -> Result<&mut MulticamSource> {
        self.multicam_source.as_mut().ok_or_else(|| {
            not_found(
                "find_multicam_source",
                "timeline has no multicam source state",
                "timeline",
                self.id,
            )
        })
    }

    /// Replaces this timeline's complete synchronized source angle state.
    pub fn set_multicam_source(
        &mut self,
        source: MulticamSource,
    ) -> Result<Option<MulticamSource>> {
        source.validate()?;
        Ok(self.multicam_source.replace(source))
    }

    /// Removes synchronized source angle state.
    pub fn remove_multicam_source(&mut self) -> Option<MulticamSource> {
        self.multicam_source.take()
    }

    /// Looks up clip-local multicam switch intent.
    #[must_use]
    pub fn multicam_clip(&self, id: ClipId) -> Option<&MulticamClip> {
        self.multicam_clips.get(&id)
    }

    /// Mutably looks up clip-local multicam switch intent inside an unpublished draft.
    pub fn multicam_clip_mut(&mut self, id: ClipId) -> Result<&mut MulticamClip> {
        self.multicam_clips.get_mut(&id).ok_or_else(|| {
            not_found(
                "find_multicam_clip",
                "multicam clip state was not found",
                "clip",
                id,
            )
        })
    }

    /// Iterates clip-local multicam state in stable clip identity order.
    pub fn multicam_clips(&self) -> impl ExactSizeIterator<Item = &MulticamClip> {
        self.multicam_clips.values()
    }

    /// Inserts or replaces switch intent for one existing ordinary nested clip.
    pub fn upsert_multicam_clip(&mut self, clip: MulticamClip) -> Result<Option<MulticamClip>> {
        let id = clip.clip_id();
        let target = find_clip(self, id).ok_or_else(|| {
            not_found(
                "upsert_multicam_clip",
                "multicam target clip was not found on this timeline",
                "clip",
                id,
            )
        })?;
        if !matches!(target.source(), ClipSource::Timeline(_)) {
            return Err(invalid(
                "upsert_multicam_clip",
                "multicam target must be an ordinary nested timeline clip",
            ));
        }
        Ok(self.multicam_clips.insert(id, clip))
    }

    /// Removes one target clip's multicam switch intent.
    pub fn remove_multicam_clip(&mut self, id: ClipId) -> Option<MulticamClip> {
        self.multicam_clips.remove(&id)
    }

    /// Returns whether exact snapping is enabled for this timeline.
    #[must_use]
    pub const fn snapping_enabled(&self) -> bool {
        self.annotations.snapping_enabled()
    }

    /// Sets the persistent timeline snapping preference.
    pub fn set_snapping_enabled(&mut self, enabled: bool) {
        self.annotations.set_snapping_enabled(enabled);
    }

    /// Looks up a marker by its stable identity.
    #[must_use]
    pub fn marker(&self, id: MarkerId) -> Option<&Marker> {
        self.annotations.marker(id)
    }

    /// Mutably looks up a marker inside an unpublished draft.
    pub fn marker_mut(&mut self, id: MarkerId) -> Result<&mut Marker> {
        self.annotations.marker_mut(id)
    }

    /// Iterates markers in stable identity order.
    pub fn markers(&self) -> impl ExactSizeIterator<Item = &Marker> {
        self.annotations.markers()
    }

    /// Inserts or replaces one marker after validating its local owner and clock.
    pub fn upsert_marker(&mut self, marker: Marker) -> Result<Option<Marker>> {
        self.annotations
            .upsert_marker(marker, self.edit_rate, &self.tracks)
    }

    /// Removes one marker and its attached metadata.
    pub fn remove_marker(&mut self, id: MarkerId) -> Option<Marker> {
        self.annotations.remove_marker(id)
    }

    /// Inserts or replaces deterministic metadata for one local owner.
    pub fn set_metadata(
        &mut self,
        owner: MetadataOwner,
        metadata: TimelineMetadata,
    ) -> Result<Option<TimelineMetadata>> {
        self.annotations.set_metadata(owner, metadata, &self.tracks)
    }

    /// Looks up deterministic metadata by local owner.
    #[must_use]
    pub fn metadata(&self, owner: MetadataOwner) -> Option<&TimelineMetadata> {
        self.annotations.metadata(owner)
    }

    /// Removes one local owner's metadata.
    pub fn remove_metadata(&mut self, owner: MetadataOwner) -> Option<TimelineMetadata> {
        self.annotations.remove_metadata(owner)
    }

    /// Resolves one marker into visible record coordinates.
    ///
    /// An object-relative range outside the owner's current duration is preserved
    /// but returns `None` until a later edit exposes it again.
    pub fn resolved_marker_range(&self, id: MarkerId) -> Result<Option<TimeRange>> {
        self.annotations
            .resolved_marker_range(id, self.edit_rate, &self.tracks)
    }

    /// Resolves the nearest exact snap target under the timeline preference.
    pub fn snap(&self, request: &SnapRequest) -> Result<Option<SnapMatch>> {
        self.annotations.snap(request, self.edit_rate, &self.tracks)
    }

    /// Sets whether ordinary clip selection follows linked clip components.
    pub fn set_linked_selection_enabled(&mut self, enabled: bool) {
        self.edit_state.set_linked_selection_enabled(enabled);
    }

    /// Sets whether commands target one existing track.
    pub fn set_track_targeted(&mut self, track_id: TrackId, targeted: bool) -> Result<()> {
        self.edit_state.set_track_targeted(track_id, targeted)
    }

    /// Sets the bounded editor lane height for one existing track.
    pub fn set_track_height(&mut self, track_id: TrackId, height: u16) -> Result<()> {
        self.edit_state.set_track_height(track_id, height)
    }

    /// Sets whether authored item changes are prevented on one existing track.
    pub fn set_track_locked(&mut self, track_id: TrackId, locked: bool) -> Result<()> {
        self.edit_state.set_track_locked(track_id, locked)
    }

    /// Sets whether ripple-style changes on other tracks keep one track synchronized.
    pub fn set_track_sync_locked(&mut self, track_id: TrackId, sync_locked: bool) -> Result<()> {
        self.edit_state.set_track_sync_locked(track_id, sync_locked)
    }

    /// Sets whether one existing audio track is suppressed from output.
    pub fn set_track_muted(&mut self, track_id: TrackId, muted: bool) -> Result<()> {
        if muted
            && self
                .track(track_id)
                .is_some_and(|track| track.kind() != TrackKind::Audio)
        {
            return Err(invalid(
                "set_track_muted",
                "mute state is supported only for audio tracks",
            ));
        }
        self.edit_state.set_track_muted(track_id, muted)
    }

    /// Sets whether one existing audio track is isolated from nonsolo audio tracks.
    pub fn set_track_solo(&mut self, track_id: TrackId, solo: bool) -> Result<()> {
        if solo
            && self
                .track(track_id)
                .is_some_and(|track| track.kind() != TrackKind::Audio)
        {
            return Err(invalid(
                "set_track_solo",
                "solo state is supported only for audio tracks",
            ));
        }
        self.edit_state.set_track_solo(track_id, solo)
    }

    /// Sets whether one existing track contributes to timeline output.
    pub fn set_track_enabled(&mut self, track_id: TrackId, enabled: bool) -> Result<()> {
        self.edit_state.set_track_enabled(track_id, enabled)
    }

    /// Updates the selected object set with related or exact-object behavior.
    pub fn update_selection<I>(
        &mut self,
        objects: I,
        update: SelectionUpdate,
        expansion: SelectionExpansion,
    ) -> Result<()>
    where
        I: IntoIterator<Item = EditorialObjectId>,
    {
        self.edit_state
            .update_selection(objects, update, expansion, &self.tracks)
    }

    /// Links two or more existing clips for synchronized ordinary selection.
    pub fn link_clips<I>(&mut self, clips: I) -> Result<()>
    where
        I: IntoIterator<Item = ClipId>,
    {
        self.edit_state.link_clips(clips, &self.tracks)
    }

    /// Removes named clips from their linked components.
    pub fn unlink_clips<I>(&mut self, clips: I) -> Result<()>
    where
        I: IntoIterator<Item = ClipId>,
    {
        self.edit_state.unlink_clips(clips, &self.tracks)
    }

    /// Groups clips as one editorial unit, including each named clip's linked component.
    pub fn group_clips<I>(&mut self, clips: I) -> Result<()>
    where
        I: IntoIterator<Item = ClipId>,
    {
        self.edit_state.group_clips(clips, &self.tracks)
    }

    /// Dissolves every clip group containing one of the named clips.
    pub fn ungroup_clips<I>(&mut self, clips: I) -> Result<()>
    where
        I: IntoIterator<Item = ClipId>,
    {
        self.edit_state.ungroup_clips(clips, &self.tracks)
    }

    /// Iterates targeted tracks in authoritative bottom-to-top timeline order.
    pub fn targeted_tracks(&self) -> impl Iterator<Item = &Track> {
        self.tracks.iter().filter(|track| {
            self.edit_state
                .track_state(track.id())
                .is_some_and(|state| state.targeted())
        })
    }

    /// Iterates targeted tracks of one media kind in bottom-to-top order.
    pub fn targeted_tracks_by_kind(&self, kind: TrackKind) -> impl Iterator<Item = &Track> {
        self.targeted_tracks()
            .filter(move |track| track.kind() == kind)
    }

    /// Resolves tracks shifted by a synchronization-sensitive operation.
    ///
    /// Explicitly edited tracks participate even when their sync lock is off.
    /// Other tracks participate only while their sync lock is enabled.
    pub fn tracks_affected_by_sync<I>(&self, explicit_tracks: I) -> Result<Vec<TrackId>>
    where
        I: IntoIterator<Item = TrackId>,
    {
        let explicit: BTreeSet<_> = explicit_tracks.into_iter().collect();
        for track_id in &explicit {
            if self.track(*track_id).is_none() {
                return Err(not_found(
                    "resolve_sync_tracks",
                    "editorial track was not found",
                    "track",
                    track_id,
                ));
            }
        }
        Ok(self
            .tracks
            .iter()
            .filter(|track| {
                explicit.contains(&track.id())
                    || self
                        .edit_state
                        .track_state(track.id())
                        .is_some_and(|state| state.sync_locked())
            })
            .map(Track::id)
            .collect())
    }

    /// Looks up a track by stable identity.
    #[must_use]
    pub fn track(&self, id: TrackId) -> Option<&Track> {
        self.tracks.iter().find(|track| track.id() == id)
    }

    /// Mutably looks up a track inside an unpublished draft.
    pub fn track_mut(&mut self, id: TrackId) -> Result<&mut Track> {
        self.tracks
            .iter_mut()
            .find(|track| track.id() == id)
            .ok_or_else(|| not_found("find_track", "editorial track was not found", "track", id))
    }

    /// Inserts one track at a canonical bottom-to-top position.
    pub fn insert_track(&mut self, position: usize, track: Track, height: u16) -> Result<()> {
        if !(MIN_TRACK_HEIGHT..=MAX_TRACK_HEIGHT).contains(&height) {
            return Err(invalid(
                "insert_track",
                "track height is outside the supported editor lane bounds",
            ));
        }
        if position > self.tracks.len() {
            return Err(invalid(
                "insert_track",
                "track position must not exceed the current track count",
            ));
        }
        if self.track(track.id()).is_some() {
            return Err(conflict(
                "insert_track",
                "duplicate track identity on timeline",
                "track",
                track.id(),
            ));
        }
        let track_id = track.id();
        self.tracks.insert(position, track);
        self.edit_state.reconcile(&self.tracks);
        self.edit_state.set_track_height(track_id, height)
    }

    /// Removes one unlocked track by stable identity.
    pub fn remove_track(&mut self, id: TrackId) -> Result<Track> {
        let position = self
            .tracks
            .iter()
            .position(|track| track.id() == id)
            .ok_or_else(|| {
                not_found("remove_track", "editorial track was not found", "track", id)
            })?;
        if self
            .edit_state
            .track_state(id)
            .is_some_and(|state| state.locked())
        {
            return Err(conflict(
                "remove_track",
                "locked tracks must be unlocked before deletion",
                "track",
                id,
            ));
        }
        let removed = self.tracks.remove(position);
        self.edit_state.reconcile(&self.tracks);
        self.annotations.reconcile(&self.tracks);
        self.reconcile_multicam_state();
        Ok(removed)
    }

    /// Moves one track to a canonical bottom-to-top final position.
    pub fn reorder_track(&mut self, id: TrackId, position: usize) -> Result<()> {
        if position >= self.tracks.len() {
            return Err(invalid(
                "reorder_track",
                "track position must identify one final position in the timeline",
            ));
        }
        let current = self
            .tracks
            .iter()
            .position(|track| track.id() == id)
            .ok_or_else(|| {
                not_found(
                    "reorder_track",
                    "editorial track was not found",
                    "track",
                    id,
                )
            })?;
        if current != position {
            let track = self.tracks.remove(current);
            self.tracks.insert(position, track);
        }
        Ok(())
    }

    /// Replaces the editor-facing timeline name inside an unpublished draft.
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    /// Replaces the external start coordinate inside an unpublished draft.
    pub fn set_global_start(&mut self, global_start: RationalTime) {
        self.global_start = global_start;
    }

    /// Replaces the complete bottom-to-top track order inside an unpublished draft.
    pub fn replace_tracks(&mut self, tracks: Vec<Track>) {
        self.tracks = tracks;
    }

    /// Returns the longest track duration.
    pub fn duration(&self) -> Result<Duration> {
        let longest = self
            .tracks
            .iter()
            .map(Track::end_time)
            .try_fold(RationalTime::zero(self.edit_rate), |longest, end| {
                end.map(|end| if end > longest { end } else { longest })
            })?;
        let longest = longest.checked_rescale(self.edit_rate, TimeRounding::Exact)?;
        let value = u64::try_from(longest.value()).map_err(|_| {
            invalid(
                "timeline_duration",
                "timeline duration must not end before timeline zero",
            )
        })?;
        Duration::new(value, self.edit_rate)
    }

    pub(crate) fn inherit_multicam_fragment(&mut self, original: ClipId, created: ClipId) {
        if let Some(state) = self.multicam_clips.get(&original).cloned() {
            self.multicam_clips
                .insert(created, state.clone_with_clip_id(created));
        }
        if let Some(source) = &mut self.multicam_source {
            source.inherit_source_fragment(original, created);
        }
    }

    pub(crate) fn transfer_multicam_clip(&mut self, removed: ClipId, inserted: ClipId) {
        if let Some(state) = self.multicam_clips.remove(&removed) {
            self.multicam_clips
                .insert(inserted, state.clone_with_clip_id(inserted));
        }
        if let Some(source) = &mut self.multicam_source {
            source.transfer_source_clip(removed, inserted);
        }
    }

    fn reconcile_multicam_state(&mut self) {
        let existing: BTreeSet<_> = self
            .tracks
            .iter()
            .flat_map(Track::items)
            .filter_map(TrackItem::as_clip)
            .map(Clip::id)
            .collect();
        self.multicam_clips
            .retain(|clip_id, _| existing.contains(clip_id));
        if let Some(source) = &mut self.multicam_source {
            source.reconcile(&existing);
        }
    }
}

/// A complete validated editorial snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EditorialProject {
    id: ProjectId,
    name: String,
    revision: u64,
    media_references: BTreeMap<MediaId, LinkedMediaReference>,
    media_library: MediaLibrary,
    timelines: BTreeMap<TimelineId, Timeline>,
}

impl EditorialProject {
    /// Creates and validates an initial revision-zero editorial project.
    pub fn new<M, T>(
        id: ProjectId,
        name: impl Into<String>,
        media_references: M,
        timelines: T,
    ) -> Result<Self>
    where
        M: IntoIterator<Item = LinkedMediaReference>,
        T: IntoIterator<Item = Timeline>,
    {
        let mut media_by_id = BTreeMap::new();
        for media in media_references {
            let media_id = media.id();
            if media_by_id.insert(media_id, media).is_some() {
                return Err(conflict(
                    "create_project",
                    "duplicate linked media identity",
                    "media",
                    media_id,
                ));
            }
        }
        let mut timelines_by_id = BTreeMap::new();
        for timeline in timelines {
            let timeline_id = timeline.id();
            if timelines_by_id.insert(timeline_id, timeline).is_some() {
                return Err(conflict(
                    "create_project",
                    "duplicate timeline identity",
                    "timeline",
                    timeline_id,
                ));
            }
        }
        let project = Self {
            id,
            name: name.into(),
            revision: 0,
            media_references: media_by_id,
            media_library: MediaLibrary::new(),
            timelines: timelines_by_id,
        };
        project.validate()?;
        Ok(project)
    }

    /// Returns the project identity.
    #[must_use]
    pub const fn id(&self) -> ProjectId {
        self.id
    }

    /// Returns the editor-facing project name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the monotonic published revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Looks up linked media by stable identity.
    #[must_use]
    pub fn media_reference(&self, id: MediaId) -> Option<&LinkedMediaReference> {
        self.media_references.get(&id)
    }

    /// Iterates linked media in stable identity order.
    pub fn media_references(&self) -> impl ExactSizeIterator<Item = &LinkedMediaReference> {
        self.media_references.values()
    }

    /// Returns manual organization and directly editable saved media queries.
    #[must_use]
    pub const fn media_library(&self) -> &MediaLibrary {
        &self.media_library
    }

    /// Evaluates one smart collection over current linked media state.
    pub fn smart_collection_members(&self, id: SmartCollectionId) -> Result<Vec<MediaId>> {
        self.media_library
            .matching_media(id, &self.media_references)
    }

    /// Looks up a timeline by stable identity.
    #[must_use]
    pub fn timeline(&self, id: TimelineId) -> Option<&Timeline> {
        self.timelines.get(&id)
    }

    /// Iterates timelines in stable identity order.
    pub fn timelines(&self) -> impl ExactSizeIterator<Item = &Timeline> {
        self.timelines.values()
    }

    /// Resolves one clip's source, record, and available ranges.
    ///
    /// Media availability remains optional. A nested timeline always exposes
    /// its current complete duration as a source range starting at zero.
    pub fn clip_range_context(&self, id: ClipId) -> Result<ClipRangeContext> {
        let clip = self
            .timelines
            .values()
            .flat_map(Timeline::tracks)
            .flat_map(Track::items)
            .filter_map(TrackItem::as_clip)
            .find(|clip| clip.id() == id)
            .ok_or_else(|| {
                not_found(
                    "find_clip_ranges",
                    "editorial clip was not found",
                    "clip",
                    id,
                )
            })?;
        let available_range = match clip.source() {
            ClipSource::Media(media_id) => self
                .media_references
                .get(&media_id)
                .ok_or_else(|| {
                    not_found(
                        "resolve_clip_ranges",
                        "clip references missing linked media",
                        "media",
                        media_id,
                    )
                })?
                .available_range(),
            ClipSource::Timeline(timeline_id) => {
                let timeline = self.timelines.get(&timeline_id).ok_or_else(|| {
                    not_found(
                        "resolve_clip_ranges",
                        "clip references missing nested timeline",
                        "timeline",
                        timeline_id,
                    )
                })?;
                Some(TimeRange::new(
                    RationalTime::zero(timeline.edit_rate()),
                    timeline.duration()?,
                )?)
            }
        };
        Ok(ClipRangeContext {
            source: clip.source(),
            ranges: clip.ranges(),
            time_map: clip.time_map().clone(),
            available_range,
        })
    }

    /// Applies one atomic edit against an expected project revision.
    ///
    /// The closure mutates only a private clone. The complete candidate is
    /// validated before publication, so an error leaves this snapshot unchanged.
    pub fn edit<F>(&mut self, expected_revision: u64, edit: F) -> Result<()>
    where
        F: FnOnce(&mut ProjectDraft) -> Result<()>,
    {
        if expected_revision != self.revision {
            return Err(conflict(
                "begin_edit",
                "editorial project revision is stale",
                "expected_revision",
                expected_revision,
            )
            .with_context(
                ErrorContext::new("superi-timeline.model", "begin_edit")
                    .with_field("actual_revision", self.revision.to_string()),
            ));
        }
        let mut draft = ProjectDraft {
            name: self.name.clone(),
            media_references: self.media_references.clone(),
            media_library: self.media_library.clone(),
            timelines: self.timelines.clone(),
        };
        edit(&mut draft)?;
        let revision = self.revision.checked_add(1).ok_or_else(|| {
            conflict(
                "commit_edit",
                "editorial project revision is exhausted",
                "revision",
                self.revision,
            )
        })?;
        let mut candidate = Self {
            id: self.id,
            name: draft.name,
            revision,
            media_references: draft.media_references,
            media_library: draft.media_library,
            timelines: draft.timelines,
        };
        candidate.reconcile_timeline_state();
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    fn reconcile_timeline_state(&mut self) {
        for timeline in self.timelines.values_mut() {
            timeline.edit_state.reconcile(&timeline.tracks);
            timeline.annotations.reconcile(&timeline.tracks);
            timeline.reconcile_multicam_state();
        }
    }

    fn validate(&self) -> Result<()> {
        require_text("validate_project", "project name", &self.name)?;
        for media in self.media_references.values() {
            media.validate()?;
        }
        self.media_library.validate(&self.media_references)?;

        let mut track_ids = BTreeSet::new();
        let mut object_ids = BTreeSet::new();
        let mut marker_ids = BTreeSet::new();
        for timeline in self.timelines.values() {
            require_text("validate_timeline", "timeline name", timeline.name())?;
            if timeline.global_start().timebase() != timeline.edit_rate() {
                return Err(invalid(
                    "validate_timeline",
                    "timeline global start must use its edit rate",
                ));
            }
            for track in timeline.tracks() {
                if !track_ids.insert(track.id()) {
                    return Err(conflict(
                        "validate_timeline",
                        "duplicate track identity",
                        "track",
                        track.id(),
                    ));
                }
                validate_track(track, &mut object_ids)?;
            }
            timeline.edit_state.validate(timeline.tracks())?;
            timeline.annotations.validate(
                timeline.edit_rate,
                timeline.tracks(),
                &mut marker_ids,
            )?;
        }
        self.validate_source_links()?;
        self.validate_nesting_cycles()?;
        self.validate_nested_ranges()?;
        self.validate_multicam()?;
        Ok(())
    }

    fn validate_source_links(&self) -> Result<()> {
        for timeline in self.timelines.values() {
            for track in timeline.tracks() {
                for item in track.items() {
                    let Some(clip) = item.as_clip() else {
                        continue;
                    };
                    match clip.source() {
                        ClipSource::Media(id) if !self.media_references.contains_key(&id) => {
                            return Err(not_found(
                                "validate_clip_source",
                                "clip references missing linked media",
                                "media",
                                id,
                            ));
                        }
                        ClipSource::Timeline(id) if !self.timelines.contains_key(&id) => {
                            return Err(not_found(
                                "validate_clip_source",
                                "clip references missing nested timeline",
                                "timeline",
                                id,
                            ));
                        }
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_nesting_cycles(&self) -> Result<()> {
        let mut visiting = BTreeSet::new();
        let mut visited = BTreeSet::new();
        for id in self.timelines.keys().copied() {
            self.visit_timeline(id, &mut visiting, &mut visited)?;
        }
        Ok(())
    }

    fn visit_timeline(
        &self,
        id: TimelineId,
        visiting: &mut BTreeSet<TimelineId>,
        visited: &mut BTreeSet<TimelineId>,
    ) -> Result<()> {
        if visited.contains(&id) {
            return Ok(());
        }
        if !visiting.insert(id) {
            return Err(conflict(
                "validate_nesting",
                "timeline nesting contains a cycle",
                "timeline",
                id,
            ));
        }
        let timeline = self.timelines.get(&id).expect("source links validated");
        for track in timeline.tracks() {
            for item in track.items() {
                if let Some(Clip {
                    source: ClipSource::Timeline(child),
                    ..
                }) = item.as_clip()
                {
                    self.visit_timeline(*child, visiting, visited)?;
                }
            }
        }
        visiting.remove(&id);
        visited.insert(id);
        Ok(())
    }

    fn validate_nested_ranges(&self) -> Result<()> {
        for timeline in self.timelines.values() {
            for track in timeline.tracks() {
                for item in track.items() {
                    let Some(clip) = item.as_clip() else {
                        continue;
                    };
                    let ClipSource::Timeline(source_timeline) = clip.source() else {
                        continue;
                    };
                    let source_range = clip.source_range();
                    let nested = self
                        .timelines
                        .get(&source_timeline)
                        .expect("source links validated");
                    if source_range.timebase() != nested.edit_rate() {
                        return Err(invalid(
                            "validate_nested_range",
                            "nested source range must use the nested timeline edit rate",
                        ));
                    }
                    if source_range.start().is_negative() {
                        return Err(invalid(
                            "validate_nested_range",
                            "nested source range must not start before timeline zero",
                        ));
                    }
                    let end = source_range.end_exclusive()?;
                    if end.value() > i64::try_from(nested.duration()?.value()).unwrap_or(i64::MAX) {
                        return Err(invalid(
                            "validate_nested_range",
                            "nested source range exceeds the nested timeline duration",
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn validate_multicam(&self) -> Result<()> {
        let mut angle_ids = BTreeSet::new();
        for timeline in self.timelines.values() {
            let Some(source) = timeline.multicam_source() else {
                continue;
            };
            source.validate()?;
            for angle in source.angles() {
                if !angle_ids.insert(angle.id()) {
                    return Err(conflict(
                        "validate_multicam",
                        "duplicate multicam angle identity across project timelines",
                        "angle",
                        angle.id(),
                    ));
                }
                let mut ranges = Vec::with_capacity(angle.source_clips().len());
                for clip_id in angle.source_clips() {
                    let clip = find_clip(timeline, *clip_id).ok_or_else(|| {
                        not_found(
                            "validate_multicam",
                            "multicam angle references a missing local source clip",
                            "clip",
                            clip_id,
                        )
                    })?;
                    if clip.record_range().timebase() != timeline.edit_rate() {
                        return Err(invalid(
                            "validate_multicam",
                            "multicam source clip must use the synchronized timeline clock",
                        ));
                    }
                    ranges.push(clip.record_range());
                }
                ranges.sort_by_key(|range| range.start().value());
                for pair in ranges.windows(2) {
                    if pair[0].end_exclusive()? > pair[1].start() {
                        return Err(invalid(
                            "validate_multicam",
                            "source clips in one multicam angle must not overlap",
                        ));
                    }
                }
            }
        }

        for timeline in self.timelines.values() {
            for state in timeline.multicam_clips() {
                let clip = find_clip(timeline, state.clip_id()).ok_or_else(|| {
                    not_found(
                        "validate_multicam",
                        "multicam target clip was not found on its owning timeline",
                        "clip",
                        state.clip_id(),
                    )
                })?;
                let ClipSource::Timeline(source_timeline_id) = clip.source() else {
                    return Err(invalid(
                        "validate_multicam",
                        "multicam target must retain an ordinary nested timeline source",
                    ));
                };
                let source_timeline = self
                    .timelines
                    .get(&source_timeline_id)
                    .expect("nested source links validated");
                let source = source_timeline.multicam_source().ok_or_else(|| {
                    invalid(
                        "validate_multicam",
                        "multicam target source timeline has no synchronized angle state",
                    )
                })?;
                let complete_source_range = TimeRange::new(
                    RationalTime::zero(source_timeline.edit_rate()),
                    source_timeline.duration()?,
                )?;
                state.validate_against(source, complete_source_range)?;
            }
        }
        Ok(())
    }

    pub(crate) fn restore_persisted_state(
        &mut self,
        revision: u64,
        media_library: MediaLibrary,
    ) -> Result<()> {
        let previous_revision = self.revision;
        let previous_library = std::mem::replace(&mut self.media_library, media_library);
        self.revision = revision;
        if let Err(error) = self.validate() {
            self.revision = previous_revision;
            self.media_library = previous_library;
            return Err(error);
        }
        Ok(())
    }
}

/// Mutable state exposed only while one project edit is unpublished.
pub struct ProjectDraft {
    name: String,
    media_references: BTreeMap<MediaId, LinkedMediaReference>,
    media_library: MediaLibrary,
    timelines: BTreeMap<TimelineId, Timeline>,
}

impl ProjectDraft {
    /// Replaces the project name.
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    /// Mutably looks up linked media.
    pub fn media_reference_mut(&mut self, id: MediaId) -> Result<&mut LinkedMediaReference> {
        self.media_references.get_mut(&id).ok_or_else(|| {
            not_found(
                "find_media_reference",
                "linked media was not found",
                "media",
                id,
            )
        })
    }

    /// Inserts or replaces linked media by stable identity.
    pub fn upsert_media_reference(
        &mut self,
        media: LinkedMediaReference,
    ) -> Option<LinkedMediaReference> {
        self.media_references.insert(media.id(), media)
    }

    /// Removes linked media. Commit fails if a clip still references it.
    pub fn remove_media_reference(&mut self, id: MediaId) -> Option<LinkedMediaReference> {
        self.media_references.remove(&id)
    }

    /// Returns manual media organization inside this unpublished draft.
    #[must_use]
    pub const fn media_library(&self) -> &MediaLibrary {
        &self.media_library
    }

    /// Mutably exposes manual organization and saved queries in this draft.
    pub fn media_library_mut(&mut self) -> &mut MediaLibrary {
        &mut self.media_library
    }

    /// Mutably looks up a timeline.
    pub fn timeline_mut(&mut self, id: TimelineId) -> Result<&mut Timeline> {
        self.timelines.get_mut(&id).ok_or_else(|| {
            not_found(
                "find_timeline",
                "editorial timeline was not found",
                "timeline",
                id,
            )
        })
    }

    /// Looks up a timeline without exposing unpublished project storage.
    #[must_use]
    pub fn timeline(&self, id: TimelineId) -> Option<&Timeline> {
        self.timelines.get(&id)
    }

    /// Inserts or replaces a timeline by stable identity.
    pub fn upsert_timeline(&mut self, timeline: Timeline) -> Option<Timeline> {
        self.timelines.insert(timeline.id(), timeline)
    }

    /// Removes a timeline. Commit fails if a nested clip still references it.
    pub fn remove_timeline(&mut self, id: TimelineId) -> Option<Timeline> {
        self.timelines.remove(&id)
    }
}

fn validate_track(track: &Track, object_ids: &mut BTreeSet<EditorialObjectId>) -> Result<()> {
    require_text("validate_track", "track name", track.name())?;
    let edit_rate = track.semantics().timebase();
    let mut prior_end = RationalTime::zero(edit_rate);
    for (index, item) in track.items().iter().enumerate() {
        if !object_ids.insert(item.id()) {
            return Err(conflict(
                "validate_track",
                "duplicate editorial object identity",
                "item",
                item.id(),
            ));
        }
        match item {
            TrackItem::Transition(transition) => {
                validate_transition(track, index, transition, edit_rate)?;
            }
            TrackItem::Clip(clip) => {
                require_text("validate_clip", "clip name", clip.name())?;
                validate_record_range(clip.record_range(), edit_rate, false)?;
                if clip.source_range().duration().rational_time()
                    != clip.record_range().duration().rational_time()
                {
                    return Err(invalid(
                        "validate_clip",
                        "clip source and record durations must represent equal rational time",
                    ));
                }
                require_contiguous(item, &mut prior_end)?;
            }
            TrackItem::Gap(gap) => {
                require_text("validate_gap", "gap name", gap.name())?;
                validate_record_range(gap.record_range(), edit_rate, true)?;
                require_contiguous(item, &mut prior_end)?;
            }
            TrackItem::Generator(generator) => {
                require_text("validate_generator", "generator name", generator.name())?;
                require_text("validate_generator", "generator kind", generator.kind())?;
                for key in generator.parameters().keys() {
                    require_text("validate_generator", "generator parameter key", key)?;
                }
                validate_record_range(generator.record_range(), edit_rate, false)?;
                require_contiguous(item, &mut prior_end)?;
            }
            TrackItem::Caption(caption) => {
                require_text("validate_caption", "caption name", caption.name())?;
                require_text("validate_caption", "caption text", caption.text())?;
                if let Some(language) = caption.language() {
                    require_text("validate_caption", "caption language", language)?;
                }
                validate_record_range(caption.record_range(), edit_rate, false)?;
                require_contiguous(item, &mut prior_end)?;
            }
        }
    }
    validate_transition_overlap(track)?;
    Ok(())
}

fn validate_record_range(range: TimeRange, edit_rate: Timebase, allow_empty: bool) -> Result<()> {
    if range.timebase() != edit_rate {
        return Err(invalid(
            "validate_record_range",
            "record range must use the track edit clock",
        ));
    }
    if range.start().is_negative() {
        return Err(invalid(
            "validate_record_range",
            "record range must not start before timeline zero",
        ));
    }
    if !allow_empty && range.is_empty() {
        return Err(invalid(
            "validate_record_range",
            "editorial item must have a nonzero record duration",
        ));
    }
    Ok(())
}

fn require_contiguous(item: &TrackItem, prior_end: &mut RationalTime) -> Result<()> {
    let range = item.record_range().expect("called only for timed items");
    if range.start() != *prior_end {
        return Err(invalid(
            "validate_track_timing",
            "track items must be contiguous; use an explicit gap for empty time",
        ));
    }
    *prior_end = range.end_exclusive()?;
    Ok(())
}

fn validate_transition(
    track: &Track,
    index: usize,
    transition: &Transition,
    edit_rate: Timebase,
) -> Result<()> {
    require_text("validate_transition", "transition name", transition.name())?;
    if transition.from_offset().timebase() != edit_rate
        || transition.to_offset().timebase() != edit_rate
    {
        return Err(invalid(
            "validate_transition",
            "transition offsets must use the track edit clock",
        ));
    }
    if transition.from_offset().is_zero() && transition.to_offset().is_zero() {
        return Err(invalid(
            "validate_transition",
            "transition must consume time from at least one endpoint",
        ));
    }
    let Some(previous) = index
        .checked_sub(1)
        .and_then(|value| track.items().get(value))
    else {
        return Err(invalid(
            "validate_transition",
            "transition must follow a timed track item",
        ));
    };
    let Some(next) = track.items().get(index + 1) else {
        return Err(invalid(
            "validate_transition",
            "transition must precede a timed track item",
        ));
    };
    let (Some(previous_range), Some(next_range)) = (previous.record_range(), next.record_range())
    else {
        return Err(invalid(
            "validate_transition",
            "adjacent transitions are not valid",
        ));
    };
    if transition.from() != previous.id() || transition.to() != next.id() {
        return Err(invalid(
            "validate_transition",
            "transition endpoints must match its adjacent track items",
        ));
    }
    if transition.from_offset().value() > previous_range.duration().value()
        || transition.to_offset().value() > next_range.duration().value()
    {
        return Err(invalid(
            "validate_transition",
            "transition offsets must fit within adjacent item durations",
        ));
    }
    Ok(())
}

fn validate_transition_overlap(track: &Track) -> Result<()> {
    for (index, item) in track.items().iter().enumerate() {
        let Some(range) = item.record_range() else {
            continue;
        };
        let incoming = index
            .checked_sub(1)
            .and_then(|value| track.items().get(value))
            .and_then(|item| match item {
                TrackItem::Transition(transition) => Some(transition.to_offset().value()),
                _ => None,
            })
            .unwrap_or(0);
        let outgoing = track
            .items()
            .get(index + 1)
            .and_then(|item| match item {
                TrackItem::Transition(transition) => Some(transition.from_offset().value()),
                _ => None,
            })
            .unwrap_or(0);
        let consumed = incoming.checked_add(outgoing).ok_or_else(|| {
            invalid(
                "validate_transition",
                "combined transition offsets exceed supported duration",
            )
        })?;
        if consumed > range.duration().value() {
            return Err(invalid(
                "validate_transition",
                "transitions at both ends of an item must not overlap",
            ));
        }
    }
    Ok(())
}

fn require_text(operation: &'static str, label: &'static str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        return Err(invalid(operation, format!("{label} must not be blank")));
    }
    Ok(())
}

fn invalid(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-timeline.model", operation))
}
fn not_found(
    operation: &'static str,
    message: impl Into<String>,
    field: &'static str,
    value: impl ToString,
) -> Error {
    Error::new(
        ErrorCategory::NotFound,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(
        ErrorContext::new("superi-timeline.model", operation).with_field(field, value.to_string()),
    )
}

fn conflict(
    operation: &'static str,
    message: impl Into<String>,
    field: &'static str,
    value: impl ToString,
) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(
        ErrorContext::new("superi-timeline.model", operation).with_field(field, value.to_string()),
    )
}

/// Immutable color state retained by a timeline item.
#[derive(Clone, Debug, Eq, Hash, PartialEq)]
pub struct TimelineColorMetadata {
    graph: GraphColorMetadata,
}

impl TimelineColorMetadata {
    /// Captures upstream graph metadata without changing source meaning.
    #[must_use]
    pub const fn from_graph(graph: GraphColorMetadata) -> Self {
        Self { graph }
    }

    /// Returns the exact graph metadata retained by this timeline value.
    #[must_use]
    pub const fn graph(&self) -> &GraphColorMetadata {
        &self.graph
    }

    /// Compiles a timeline item into graph metadata deterministically.
    #[must_use]
    pub fn compile(&self) -> GraphColorMetadata {
        self.graph.clone()
    }
}
