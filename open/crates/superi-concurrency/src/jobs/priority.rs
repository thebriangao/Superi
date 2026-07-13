//! Deterministic priority policy for scheduled media work.
//!
//! The scheduler owns ordering, not execution. It keeps one FIFO queue per user-visible priority
//! and dispatches from a fixed weighted service cycle. The bounded worker pool consumes
//! [`ScheduledJob`] values and applies the same service cycle across local queues without hiding
//! priority, source identity, or derived-media fallback decisions inside the thread runtime.

use std::collections::{BTreeSet, VecDeque};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::{CacheId, JobId, MediaId};

/// The kind of media work represented by one scheduled job.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum JobKind {
    /// Produce one complete video frame.
    Frame,
    /// Produce one independently schedulable image tile.
    Tile,
    /// Produce replaceable optimized or proxy media.
    Proxy,
    /// Analyze media without changing authoritative project state.
    Analysis,
    /// Populate or refresh replaceable cached data.
    Cache,
    /// Produce user-requested delivery output.
    Export,
}

impl JobKind {
    /// Every work kind defined by this version in stable code order.
    pub const ALL: &'static [Self] = &[
        Self::Frame,
        Self::Tile,
        Self::Proxy,
        Self::Analysis,
        Self::Cache,
        Self::Export,
    ];

    /// Returns the stable diagnostic and API code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Frame => "frame",
            Self::Tile => "tile",
            Self::Proxy => "proxy",
            Self::Analysis => "analysis",
            Self::Cache => "cache",
            Self::Export => "export",
        }
    }

    /// Looks up a work kind by its stable code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "frame" => Some(Self::Frame),
            "tile" => Some(Self::Tile),
            "proxy" => Some(Self::Proxy),
            "analysis" => Some(Self::Analysis),
            "cache" => Some(Self::Cache),
            "export" => Some(Self::Export),
            _ => None,
        }
    }
}

/// User-visible scheduling importance for one job.
///
/// The codes and ranks match media-operation intent. Service weights are deliberately separate
/// from rank: rank communicates urgency, while the weighted cycle guarantees that continuously
/// queued lower priorities still receive bounded service.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
#[non_exhaustive]
pub enum JobPriority {
    /// Replaceable previews, analysis, cache, and other unattended work.
    Background = 0,
    /// A user-requested offline render or export.
    Export = 1,
    /// Time-sensitive playback and prefetch work.
    Playback = 2,
    /// Direct user interaction such as a seek or requested frame.
    Interactive = 3,
}

impl JobPriority {
    /// Every priority defined by this version in ascending scheduling order.
    pub const ALL: &'static [Self] = &[
        Self::Background,
        Self::Export,
        Self::Playback,
        Self::Interactive,
    ];

    /// Returns the stable diagnostic and API code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Background => "background",
            Self::Export => "export",
            Self::Playback => "playback",
            Self::Interactive => "interactive",
        }
    }

    /// Looks up a priority by its stable code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "background" => Some(Self::Background),
            "export" => Some(Self::Export),
            "playback" => Some(Self::Playback),
            "interactive" => Some(Self::Interactive),
            _ => None,
        }
    }

    /// Returns the stable ascending urgency rank.
    #[must_use]
    pub const fn rank(self) -> u8 {
        self as u8
    }

    /// Returns this priority's dispatches in one saturated 15-job service cycle.
    #[must_use]
    pub const fn service_weight(self) -> u8 {
        match self {
            Self::Background => 1,
            Self::Export => 2,
            Self::Playback => 4,
            Self::Interactive => 8,
        }
    }

    /// Returns the maximum other dispatches between services while this priority stays queued.
    ///
    /// Empty priority classes are skipped, so this is a saturated upper bound rather than a delay
    /// imposed when the scheduler has less work.
    #[must_use]
    pub const fn maximum_waiting_dispatches(self) -> u8 {
        match self {
            Self::Background => 14,
            Self::Export => 7,
            Self::Playback => 3,
            Self::Interactive => 1,
        }
    }
}

/// The requested quality for replaceable derived media.
///
/// Ordering is low to high quality. A fallback policy may select only a lower value than the
/// request, never a higher value that could violate the caller's resource choice.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
#[non_exhaustive]
pub enum DerivedQuality {
    /// One eighth of the full linear dimensions.
    Eighth = 0,
    /// One quarter of the full linear dimensions.
    Quarter = 1,
    /// One half of the full linear dimensions.
    Half = 2,
    /// Full source dimensions and requested processing quality.
    Full = 3,
}

impl DerivedQuality {
    /// Every quality defined by this version in ascending quality order.
    pub const ALL: &'static [Self] = &[Self::Eighth, Self::Quarter, Self::Half, Self::Full];

    /// Returns the stable diagnostic and API code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Eighth => "eighth",
            Self::Quarter => "quarter",
            Self::Half => "half",
            Self::Full => "full",
        }
    }

    /// Looks up a quality by its stable code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "eighth" => Some(Self::Eighth),
            "quarter" => Some(Self::Quarter),
            "half" => Some(Self::Half),
            "full" => Some(Self::Full),
            _ => None,
        }
    }

    /// Returns the stable ascending quality rank.
    #[must_use]
    pub const fn rank(self) -> u8 {
        self as u8
    }
}

/// Policy for choosing replaceable derived media.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum DerivedFallbackPolicy {
    /// Use the requested fresh quality when present, otherwise use the source.
    ExactOrSource,
    /// Use the nearest fresh lower quality when exact quality is absent, then use the source.
    LowerQualityOrSource,
    /// Bypass every derived candidate and use the source.
    SourceOnly,
}

impl DerivedFallbackPolicy {
    /// Every policy defined by this version in stable code order.
    pub const ALL: &'static [Self] = &[
        Self::ExactOrSource,
        Self::LowerQualityOrSource,
        Self::SourceOnly,
    ];

    /// Returns the stable diagnostic and API code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::ExactOrSource => "exact_or_source",
            Self::LowerQualityOrSource => "lower_quality_or_source",
            Self::SourceOnly => "source_only",
        }
    }

    /// Looks up a policy by its stable code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "exact_or_source" => Some(Self::ExactOrSource),
            "lower_quality_or_source" => Some(Self::LowerQualityOrSource),
            "source_only" => Some(Self::SourceOnly),
            _ => None,
        }
    }
}

/// One available derived-media candidate.
///
/// A candidate never replaces source identity. It names its own replaceable cache identity and the
/// exact source revision from which it was produced.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DerivedMediaCandidate {
    cache_id: CacheId,
    source: MediaId,
    source_revision: u64,
    quality: DerivedQuality,
}

impl DerivedMediaCandidate {
    /// Creates a candidate tied to one exact source revision and quality.
    #[must_use]
    pub const fn new(
        cache_id: CacheId,
        source: MediaId,
        source_revision: u64,
        quality: DerivedQuality,
    ) -> Self {
        Self {
            cache_id,
            source,
            source_revision,
            quality,
        }
    }

    /// Returns the replaceable cache identity.
    #[must_use]
    pub const fn cache_id(self) -> CacheId {
        self.cache_id
    }

    /// Returns the authoritative source identity.
    #[must_use]
    pub const fn source(self) -> MediaId {
        self.source
    }

    /// Returns the exact source revision used to create this candidate.
    #[must_use]
    pub const fn source_revision(self) -> u64 {
        self.source_revision
    }

    /// Returns the candidate quality.
    #[must_use]
    pub const fn quality(self) -> DerivedQuality {
        self.quality
    }
}

/// Why a derived-media request selected its result.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum DerivedSelectionReason {
    /// A fresh candidate matched the requested quality.
    Exact,
    /// Exact quality was absent and the nearest fresh lower quality was selected.
    LowerQuality,
    /// The request explicitly required authoritative source media.
    SourceOnlyPolicy,
    /// No candidate matched both source identity and source revision.
    NoFreshCandidate,
    /// Fresh candidates existed, but none satisfied the requested quality policy.
    RequestedQualityUnavailable,
}

impl DerivedSelectionReason {
    /// Returns the stable diagnostic and API code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::LowerQuality => "lower_quality",
            Self::SourceOnlyPolicy => "source_only_policy",
            Self::NoFreshCandidate => "no_fresh_candidate",
            Self::RequestedQualityUnavailable => "requested_quality_unavailable",
        }
    }
}

/// An explicit request for source or replaceable derived media.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DerivedMediaRequest {
    source: MediaId,
    source_revision: u64,
    quality: DerivedQuality,
    fallback: DerivedFallbackPolicy,
}

impl DerivedMediaRequest {
    /// Creates a request for one exact source revision and quality policy.
    #[must_use]
    pub const fn new(
        source: MediaId,
        source_revision: u64,
        quality: DerivedQuality,
        fallback: DerivedFallbackPolicy,
    ) -> Self {
        Self {
            source,
            source_revision,
            quality,
            fallback,
        }
    }

    /// Returns the authoritative source identity.
    #[must_use]
    pub const fn source(self) -> MediaId {
        self.source
    }

    /// Returns the required source revision.
    #[must_use]
    pub const fn source_revision(self) -> u64 {
        self.source_revision
    }

    /// Returns the requested quality.
    #[must_use]
    pub const fn quality(self) -> DerivedQuality {
        self.quality
    }

    /// Returns the fallback policy.
    #[must_use]
    pub const fn fallback(self) -> DerivedFallbackPolicy {
        self.fallback
    }

    /// Selects one candidate deterministically, or explicitly selects the source.
    ///
    /// Wrong-source and stale candidates are never eligible. Equal-quality candidates use the
    /// lowest stable cache identity so input iteration order cannot change the outcome.
    #[must_use]
    pub fn select(self, candidates: &[DerivedMediaCandidate]) -> DerivedMediaSelection {
        if self.fallback == DerivedFallbackPolicy::SourceOnly {
            return DerivedMediaSelection::from_source(
                self,
                DerivedSelectionReason::SourceOnlyPolicy,
            );
        }

        let mut has_fresh_candidate = false;
        let mut exact: Option<DerivedMediaCandidate> = None;
        let mut lower: Option<DerivedMediaCandidate> = None;

        for candidate in candidates.iter().copied().filter(|candidate| {
            candidate.source == self.source && candidate.source_revision == self.source_revision
        }) {
            has_fresh_candidate = true;
            if candidate.quality == self.quality {
                let replaces_exact = match exact {
                    Some(selected) => candidate.cache_id < selected.cache_id,
                    None => true,
                };
                if replaces_exact {
                    exact = Some(candidate);
                }
                continue;
            }

            let replaces_lower = match lower {
                Some(selected) => {
                    candidate.quality > selected.quality
                        || (candidate.quality == selected.quality
                            && candidate.cache_id < selected.cache_id)
                }
                None => true,
            };
            if self.fallback == DerivedFallbackPolicy::LowerQualityOrSource
                && candidate.quality < self.quality
                && replaces_lower
            {
                lower = Some(candidate);
            }
        }

        if let Some(candidate) = exact {
            return DerivedMediaSelection::derived(self, candidate, DerivedSelectionReason::Exact);
        }
        if let Some(candidate) = lower {
            return DerivedMediaSelection::derived(
                self,
                candidate,
                DerivedSelectionReason::LowerQuality,
            );
        }

        let reason = if has_fresh_candidate {
            DerivedSelectionReason::RequestedQualityUnavailable
        } else {
            DerivedSelectionReason::NoFreshCandidate
        };
        DerivedMediaSelection::from_source(self, reason)
    }
}

/// The transparent outcome of a derived-media selection.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct DerivedMediaSelection {
    source: MediaId,
    source_revision: u64,
    requested_quality: DerivedQuality,
    candidate: Option<DerivedMediaCandidate>,
    reason: DerivedSelectionReason,
}

impl DerivedMediaSelection {
    const fn from_source(request: DerivedMediaRequest, reason: DerivedSelectionReason) -> Self {
        Self {
            source: request.source,
            source_revision: request.source_revision,
            requested_quality: request.quality,
            candidate: None,
            reason,
        }
    }

    const fn derived(
        request: DerivedMediaRequest,
        candidate: DerivedMediaCandidate,
        reason: DerivedSelectionReason,
    ) -> Self {
        Self {
            source: request.source,
            source_revision: request.source_revision,
            requested_quality: request.quality,
            candidate: Some(candidate),
            reason,
        }
    }

    /// Returns the authoritative source identity, even when derived media was selected.
    #[must_use]
    pub const fn source(self) -> MediaId {
        self.source
    }

    /// Returns the required source revision, even when derived media was selected.
    #[must_use]
    pub const fn source_revision(self) -> u64 {
        self.source_revision
    }

    /// Returns the quality requested by the caller.
    #[must_use]
    pub const fn requested_quality(self) -> DerivedQuality {
        self.requested_quality
    }

    /// Returns the replaceable cache identity when derived media was selected.
    #[must_use]
    pub fn cache_id(self) -> Option<CacheId> {
        self.candidate.map(DerivedMediaCandidate::cache_id)
    }

    /// Returns the selected derived quality, or `None` when source media was selected.
    #[must_use]
    pub fn quality(self) -> Option<DerivedQuality> {
        self.candidate.map(DerivedMediaCandidate::quality)
    }

    /// Returns why this result was selected.
    #[must_use]
    pub const fn reason(self) -> DerivedSelectionReason {
        self.reason
    }

    /// Returns whether replaceable derived media was selected.
    #[must_use]
    pub const fn is_derived(self) -> bool {
        self.candidate.is_some()
    }

    /// Returns whether the selected derived media exactly matches requested quality.
    #[must_use]
    pub const fn is_exact(self) -> bool {
        matches!(self.reason, DerivedSelectionReason::Exact)
    }
}

/// One payload and its explicit scheduling intent.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ScheduledJob<T> {
    id: JobId,
    kind: JobKind,
    priority: JobPriority,
    derived_media: Option<DerivedMediaRequest>,
    payload: T,
}

impl<T> ScheduledJob<T> {
    /// Creates a scheduled job without derived-media intent.
    #[must_use]
    pub const fn new(id: JobId, kind: JobKind, priority: JobPriority, payload: T) -> Self {
        Self {
            id,
            kind,
            priority,
            derived_media: None,
            payload,
        }
    }

    /// Attaches transparent derived-media intent.
    #[must_use]
    pub const fn with_derived_media(mut self, request: DerivedMediaRequest) -> Self {
        self.derived_media = Some(request);
        self
    }

    /// Returns the stable job identity.
    #[must_use]
    pub const fn id(&self) -> JobId {
        self.id
    }

    /// Returns the work kind.
    #[must_use]
    pub const fn kind(&self) -> JobKind {
        self.kind
    }

    /// Returns the user-visible scheduling priority.
    #[must_use]
    pub const fn priority(&self) -> JobPriority {
        self.priority
    }

    /// Returns derived-media intent when the work can produce or consume it.
    #[must_use]
    pub const fn derived_media(&self) -> Option<&DerivedMediaRequest> {
        self.derived_media.as_ref()
    }

    /// Returns the opaque execution payload.
    #[must_use]
    pub const fn payload(&self) -> &T {
        &self.payload
    }

    /// Consumes the scheduling envelope and returns its execution payload.
    #[must_use]
    pub fn into_payload(self) -> T {
        self.payload
    }
}

pub(super) const SERVICE_PATTERN: [JobPriority; 15] = [
    JobPriority::Interactive,
    JobPriority::Playback,
    JobPriority::Interactive,
    JobPriority::Export,
    JobPriority::Interactive,
    JobPriority::Playback,
    JobPriority::Interactive,
    JobPriority::Background,
    JobPriority::Interactive,
    JobPriority::Playback,
    JobPriority::Interactive,
    JobPriority::Export,
    JobPriority::Interactive,
    JobPriority::Playback,
    JobPriority::Interactive,
];

/// Deterministic weighted FIFO scheduler for media jobs.
///
/// This type is intentionally synchronous and does not execute payloads. One owner may place it
/// behind the bounded channel or lock chosen by later execution-domain and worker-pool checkpoints.
#[derive(Clone, Debug)]
pub struct PriorityScheduler<T> {
    queues: [VecDeque<ScheduledJob<T>>; 4],
    queued_ids: BTreeSet<JobId>,
    service_cursor: usize,
}

impl<T> PriorityScheduler<T> {
    /// Creates an empty scheduler at the start of its deterministic service cycle.
    #[must_use]
    pub fn new() -> Self {
        Self {
            queues: std::array::from_fn(|_| VecDeque::new()),
            queued_ids: BTreeSet::new(),
            service_cursor: 0,
        }
    }

    /// Enqueues one job at the tail of its priority class.
    ///
    /// Duplicate queued identity is rejected so dependency and result layers can refer to exactly
    /// one waiting job. The identity may be reused after dispatch begins a new lifecycle.
    pub fn enqueue(&mut self, job: ScheduledJob<T>) -> Result<()> {
        if !self.queued_ids.insert(job.id) {
            return Err(Error::new(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "job identity is already queued",
            )
            .with_context(
                ErrorContext::new("superi-concurrency.jobs", "enqueue")
                    .with_field("job_id", job.id.to_string())
                    .with_field("priority", job.priority.code()),
            ));
        }

        self.queues[usize::from(job.priority.rank())].push_back(job);
        Ok(())
    }

    /// Dispatches the next job according to the weighted service cycle.
    ///
    /// Empty classes are skipped immediately. FIFO ordering within a class never changes.
    pub fn next_job(&mut self) -> Option<ScheduledJob<T>> {
        if self.is_empty() {
            return None;
        }

        for _ in 0..SERVICE_PATTERN.len() {
            let priority = SERVICE_PATTERN[self.service_cursor];
            self.service_cursor = (self.service_cursor + 1) % SERVICE_PATTERN.len();
            if let Some(job) = self.queues[usize::from(priority.rank())].pop_front() {
                let removed = self.queued_ids.remove(&job.id);
                debug_assert!(
                    removed,
                    "queued job identity index must match FIFO contents"
                );
                return Some(job);
            }
        }

        debug_assert!(
            false,
            "a nonempty scheduler must have a queued priority class"
        );
        None
    }

    /// Dispatches the oldest job from one exact priority class.
    ///
    /// The worker pool owns the global weighted cursor when several local schedulers participate
    /// in one pool. Keeping this operation crate-private prevents public callers from bypassing the
    /// scheduler's starvation bounds.
    pub(super) fn next_job_for(&mut self, priority: JobPriority) -> Option<ScheduledJob<T>> {
        let job = self.queues[usize::from(priority.rank())].pop_front()?;
        let removed = self.queued_ids.remove(&job.id);
        debug_assert!(
            removed,
            "queued job identity index must match FIFO contents"
        );
        Some(job)
    }

    /// Returns the total number of queued jobs.
    #[must_use]
    pub fn len(&self) -> usize {
        self.queued_ids.len()
    }

    /// Returns the number of queued jobs in one priority class.
    #[must_use]
    pub fn len_for(&self, priority: JobPriority) -> usize {
        self.queues[usize::from(priority.rank())].len()
    }

    /// Returns whether no job is queued.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.queued_ids.is_empty()
    }
}

impl<T> Default for PriorityScheduler<T> {
    fn default() -> Self {
        Self::new()
    }
}
