//! Native semantic values shared by editorial tracks.
//!
//! This module owns the distinctions between video, audio, caption, and data
//! tracks. Project, timeline, track, and clip containers are separate editorial
//! objects. The semantic values here are designed to be embedded by those
//! objects without creating another identity or time model.

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::{ClipId, TrackId};
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::time::{
    Duration, FrameRate, RationalTime, SampleTime, TimeRange, TimeRounding, Timebase,
};

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
