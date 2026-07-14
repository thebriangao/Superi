//! Starvation and deadlock diagnostics for explicit scheduling ownership.
//!
//! [`LivenessProbe`] lets UI, playback, audio, and worker paths publish idle,
//! runnable, waiting, and progress state through one atomic word. Registration,
//! resource ownership, wait relationships, sampling, graph analysis, and event
//! construction belong to the observer side and may allocate or lock.
//!
//! A deadlock finding is conservative. Every edge in a reported cycle names a
//! waiting actor, the resource it needs, and that resource's registered owner.
//! A wait on an unowned resource remains visible but is never called a deadlock.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::marker::PhantomData;
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use superi_core::diagnostics::{DiagnosticEvent, DiagnosticSeverity, TraceField};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::JobId;

use crate::jobs::{JobKind, JobPriority};
use crate::threads::ExecutionDomain;

const COMPONENT: &str = "superi-concurrency.diagnostics";
const ACTIVITY_BITS: u32 = 2;
const ACTIVITY_MASK: u64 = (1 << ACTIVITY_BITS) - 1;
const MAX_REVISION: u64 = u64::MAX >> ACTIVITY_BITS;

/// One scheduling participant whose progress can be observed.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum LivenessActor {
    /// The single owner or platform callback for an execution domain.
    ExecutionDomain(ExecutionDomain),
    /// One typed scheduled job.
    Job(JobId),
}

impl LivenessActor {
    /// Creates an execution-domain actor.
    #[must_use]
    pub const fn domain(domain: ExecutionDomain) -> Self {
        Self::ExecutionDomain(domain)
    }

    /// Creates a scheduled-job actor.
    #[must_use]
    pub const fn job(job_id: JobId) -> Self {
        Self::Job(job_id)
    }

    /// Returns the stable actor-kind code.
    #[must_use]
    pub const fn kind_code(self) -> &'static str {
        match self {
            Self::ExecutionDomain(_) => "execution_domain",
            Self::Job(_) => "job",
        }
    }
}

impl fmt::Display for LivenessActor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ExecutionDomain(domain) => write!(formatter, "domain:{domain}"),
            Self::Job(job_id) => job_id.fmt(formatter),
        }
    }
}

/// Explicit actor activity used to distinguish quiescence from starvation.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
#[non_exhaustive]
pub enum LivenessActivity {
    /// The actor has no runnable work and must not be considered starved.
    Idle = 0,
    /// The actor has runnable work and is expected to report progress.
    Runnable = 1,
    /// The actor cannot proceed until its registered wait completes.
    Waiting = 2,
}

impl LivenessActivity {
    /// Every activity in stable code order.
    pub const ALL: &'static [Self] = &[Self::Idle, Self::Runnable, Self::Waiting];

    /// Returns the stable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Runnable => "runnable",
            Self::Waiting => "waiting",
        }
    }

    const fn from_encoded(encoded: u64) -> Self {
        match encoded & ACTIVITY_MASK {
            1 => Self::Runnable,
            2 => Self::Waiting,
            _ => Self::Idle,
        }
    }
}

impl fmt::Display for LivenessActivity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// Validated stable identity for a resource that can have one wait owner.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct WaitResource(String);

impl WaitResource {
    /// Creates a canonical resource identity.
    ///
    /// Resource names use lowercase ASCII letters and digits separated by
    /// `.`, `_`, or `-`. Separators may not lead, trail, or repeat.
    pub fn new(name: impl Into<String>) -> Result<Self> {
        let name = name.into();
        if !is_canonical_name(&name) {
            return Err(diagnostic_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "wait resource name is not canonical",
                "create_resource",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "resource_name").with_field("resource", name),
            ));
        }
        Ok(Self(name))
    }

    /// Returns the canonical resource identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for WaitResource {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Immutable actor metadata and its starvation threshold.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LivenessRegistration {
    actor: LivenessActor,
    domain: ExecutionDomain,
    job_kind: Option<JobKind>,
    priority: Option<JobPriority>,
    starvation_threshold: Duration,
}

impl LivenessRegistration {
    /// Describes one execution-domain owner or platform callback.
    #[must_use]
    pub const fn domain(domain: ExecutionDomain, starvation_threshold: Duration) -> Self {
        Self {
            actor: LivenessActor::domain(domain),
            domain,
            job_kind: None,
            priority: None,
            starvation_threshold,
        }
    }

    /// Describes one scheduled job in the background-job execution domain.
    #[must_use]
    pub const fn job(
        job_id: JobId,
        job_kind: JobKind,
        priority: JobPriority,
        starvation_threshold: Duration,
    ) -> Self {
        Self {
            actor: LivenessActor::job(job_id),
            domain: ExecutionDomain::BackgroundJob,
            job_kind: Some(job_kind),
            priority: Some(priority),
            starvation_threshold,
        }
    }

    /// Validates this registration before monitor insertion.
    pub fn validate(&self) -> Result<()> {
        if self.starvation_threshold.is_zero() {
            return Err(diagnostic_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "starvation threshold must be greater than zero",
                "validate_registration",
            )
            .with_context(actor_context(self.actor)));
        }
        Ok(())
    }

    /// Returns the typed actor identity.
    #[must_use]
    pub const fn actor(&self) -> LivenessActor {
        self.actor
    }

    /// Returns the actor's execution ownership domain.
    #[must_use]
    pub const fn execution_domain(&self) -> ExecutionDomain {
        self.domain
    }

    /// Returns the job kind when this actor is a scheduled job.
    #[must_use]
    pub const fn job_kind(&self) -> Option<JobKind> {
        self.job_kind
    }

    /// Returns the job priority when this actor is a scheduled job.
    #[must_use]
    pub const fn priority(&self) -> Option<JobPriority> {
        self.priority
    }

    /// Returns the maximum no-progress interval before starvation is reported.
    #[must_use]
    pub const fn starvation_threshold(&self) -> Duration {
        self.starvation_threshold
    }
}

struct ProbeState {
    encoded: AtomicU64,
}

impl ProbeState {
    const fn new() -> Self {
        Self {
            encoded: AtomicU64::new(encode_probe(LivenessActivity::Idle, 0)),
        }
    }

    fn snapshot(&self) -> ProbeSnapshot {
        decode_probe(self.encoded.load(Ordering::Acquire))
    }

    fn mark_activity(&self, activity: LivenessActivity) {
        let mut current = self.encoded.load(Ordering::Relaxed);
        loop {
            let next = (current & !ACTIVITY_MASK) | activity as u64;
            match self.encoded.compare_exchange_weak(
                current,
                next,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => return,
                Err(observed) => current = observed,
            }
        }
    }

    fn record_progress(&self, actor: LivenessActor) -> Result<u64> {
        let mut current = self.encoded.load(Ordering::Relaxed);
        loop {
            let snapshot = decode_probe(current);
            if snapshot.revision == MAX_REVISION {
                return Err(diagnostic_error(
                    ErrorCategory::ResourceExhausted,
                    Recoverability::Terminal,
                    "liveness progress revision is exhausted",
                    "record_progress",
                )
                .with_context(actor_context(actor)));
            }
            let next = encode_probe(snapshot.activity, snapshot.revision + 1);
            match self.encoded.compare_exchange_weak(
                current,
                next,
                Ordering::Release,
                Ordering::Relaxed,
            ) {
                Ok(_) => return Ok(snapshot.revision + 1),
                Err(observed) => current = observed,
            }
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct ProbeSnapshot {
    activity: LivenessActivity,
    revision: u64,
}

/// Lock-free progress publisher for one registered actor.
///
/// Marking activity and recording ordinary progress touch one atomic word and
/// perform no allocation or locking on the successful path. Monitor sampling
/// and wait-graph mutation are separate observer-side operations.
#[derive(Clone)]
pub struct LivenessProbe {
    actor: LivenessActor,
    state: Arc<ProbeState>,
}

impl LivenessProbe {
    /// Returns the typed actor identity.
    #[must_use]
    pub const fn actor(&self) -> LivenessActor {
        self.actor
    }

    /// Returns the current published activity.
    #[must_use]
    pub fn activity(&self) -> LivenessActivity {
        self.state.snapshot().activity
    }

    /// Returns the current monotonic progress revision.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.state.snapshot().revision
    }

    /// Marks the actor as having runnable work.
    pub fn mark_runnable(&self) {
        self.state.mark_activity(LivenessActivity::Runnable);
    }

    /// Marks the actor as quiescent so elapsed time cannot be called starvation.
    pub fn mark_idle(&self) {
        self.state.mark_activity(LivenessActivity::Idle);
    }

    /// Records one unit of observable progress and returns the new revision.
    pub fn record_progress(&self) -> Result<u64> {
        self.state.record_progress(self.actor)
    }
}

impl fmt::Debug for LivenessProbe {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let snapshot = self.state.snapshot();
        formatter
            .debug_struct("LivenessProbe")
            .field("actor", &self.actor)
            .field("activity", &snapshot.activity)
            .field("revision", &snapshot.revision)
            .finish()
    }
}

struct ActorEntry {
    registration: LivenessRegistration,
    probe: Arc<ProbeState>,
    observed_activity: LivenessActivity,
    observed_revision: u64,
    observed_at: Instant,
}

#[derive(Clone, Debug)]
struct WaitState {
    resource: WaitResource,
    since: Instant,
    token: u64,
    previous_activity: LivenessActivity,
}

struct MonitorState {
    actors: BTreeMap<LivenessActor, ActorEntry>,
    resources: BTreeMap<WaitResource, Option<LivenessActor>>,
    waits: BTreeMap<LivenessActor, WaitState>,
    next_wait_token: u64,
}

impl MonitorState {
    fn new() -> Self {
        Self {
            actors: BTreeMap::new(),
            resources: BTreeMap::new(),
            waits: BTreeMap::new(),
            next_wait_token: 1,
        }
    }
}

struct MonitorInner {
    state: Mutex<MonitorState>,
}

/// Observer-side registry for liveness, ownership, and wait relationships.
#[derive(Clone)]
pub struct LivenessMonitor {
    inner: Arc<MonitorInner>,
}

impl LivenessMonitor {
    /// Creates an empty monitor.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Arc::new(MonitorInner {
                state: Mutex::new(MonitorState::new()),
            }),
        }
    }

    /// Registers one actor using the current monotonic time as its baseline.
    pub fn register(&self, registration: LivenessRegistration) -> Result<LivenessProbe> {
        self.register_at(registration, Instant::now())
    }

    /// Registers one actor using an explicit monotonic observation baseline.
    pub fn register_at(
        &self,
        registration: LivenessRegistration,
        observed_at: Instant,
    ) -> Result<LivenessProbe> {
        registration.validate()?;
        let actor = registration.actor();
        let probe = Arc::new(ProbeState::new());
        let mut state = lock_unpoisoned(&self.inner.state);
        if state.actors.contains_key(&actor) {
            return Err(diagnostic_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "liveness actor is already registered",
                "register_actor",
            )
            .with_context(actor_context(actor)));
        }
        state.actors.insert(
            actor,
            ActorEntry {
                registration,
                probe: Arc::clone(&probe),
                observed_activity: LivenessActivity::Idle,
                observed_revision: 0,
                observed_at,
            },
        );
        Ok(LivenessProbe {
            actor,
            state: probe,
        })
    }

    /// Registers a resource and its current owner.
    pub fn register_resource(&self, resource: WaitResource, owner: LivenessActor) -> Result<()> {
        let mut state = lock_unpoisoned(&self.inner.state);
        require_actor(&state, owner, "register_resource")?;
        if state.resources.contains_key(&resource) {
            return Err(resource_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "wait resource is already registered",
                "register_resource",
                &resource,
            ));
        }
        state.resources.insert(resource, Some(owner));
        Ok(())
    }

    /// Transfers one registered resource to another registered actor.
    pub fn set_resource_owner(&self, resource: &WaitResource, owner: LivenessActor) -> Result<()> {
        let mut state = lock_unpoisoned(&self.inner.state);
        require_actor(&state, owner, "set_resource_owner")?;
        let registered_owner = state.resources.get_mut(resource).ok_or_else(|| {
            resource_error(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "wait resource is not registered",
                "set_resource_owner",
                resource,
            )
        })?;
        *registered_owner = Some(owner);
        Ok(())
    }

    /// Makes a registered resource explicitly unowned.
    pub fn release_resource(&self, resource: &WaitResource) -> Result<()> {
        let mut state = lock_unpoisoned(&self.inner.state);
        let owner = state.resources.get_mut(resource).ok_or_else(|| {
            resource_error(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "wait resource is not registered",
                "release_resource",
                resource,
            )
        })?;
        *owner = None;
        Ok(())
    }

    /// Begins one wait using the current monotonic time.
    ///
    /// The returned thread-bound guard keeps the relationship active until it
    /// drops. Callers must create it before the actual wait begins.
    pub fn begin_wait(
        &self,
        actor: LivenessActor,
        resource: WaitResource,
    ) -> Result<LivenessWaitGuard> {
        self.begin_wait_at(actor, resource, Instant::now())
    }

    /// Begins one wait using an explicit monotonic start time.
    pub fn begin_wait_at(
        &self,
        actor: LivenessActor,
        resource: WaitResource,
        since: Instant,
    ) -> Result<LivenessWaitGuard> {
        let mut state = lock_unpoisoned(&self.inner.state);
        let probe = require_actor(&state, actor, "begin_wait")?.probe.clone();
        if !state.resources.contains_key(&resource) {
            return Err(resource_error(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "wait resource is not registered",
                "begin_wait",
                &resource,
            ));
        }
        if state.waits.contains_key(&actor) {
            return Err(diagnostic_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "liveness actor already has an active wait",
                "begin_wait",
            )
            .with_context(actor_context(actor)));
        }
        let token = state.next_wait_token;
        state.next_wait_token = state.next_wait_token.checked_add(1).ok_or_else(|| {
            diagnostic_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::Terminal,
                "liveness wait token is exhausted",
                "begin_wait",
            )
        })?;
        let previous_activity = probe.snapshot().activity;
        probe.mark_activity(LivenessActivity::Waiting);
        state.waits.insert(
            actor,
            WaitState {
                resource: resource.clone(),
                since,
                token,
                previous_activity,
            },
        );
        Ok(LivenessWaitGuard {
            monitor: self.clone(),
            actor,
            resource,
            token,
            thread_bound: PhantomData,
        })
    }

    /// Samples the monitor using the current monotonic time.
    #[must_use]
    pub fn snapshot(&self) -> LivenessSnapshot {
        self.snapshot_at(Instant::now())
    }

    /// Samples actor progress and analyzes waits using an explicit monotonic time.
    #[must_use]
    pub fn snapshot_at(&self, observed_at: Instant) -> LivenessSnapshot {
        let mut state = lock_unpoisoned(&self.inner.state);
        let mut actors = Vec::with_capacity(state.actors.len());
        for (actor, entry) in &mut state.actors {
            let probe = entry.probe.snapshot();
            if probe.activity != entry.observed_activity
                || probe.revision != entry.observed_revision
            {
                entry.observed_activity = probe.activity;
                entry.observed_revision = probe.revision;
                entry.observed_at = observed_at;
            }
            actors.push(ActorLivenessSnapshot {
                actor: *actor,
                domain: entry.registration.execution_domain(),
                job_kind: entry.registration.job_kind(),
                priority: entry.registration.priority(),
                activity: probe.activity,
                progress_revision: probe.revision,
                stalled_for: elapsed(observed_at, entry.observed_at),
                starvation_threshold: entry.registration.starvation_threshold(),
            });
        }

        let waits = state
            .waits
            .iter()
            .map(|(actor, wait)| WaitSnapshot {
                waiter: *actor,
                resource: wait.resource.clone(),
                owner: state.resources.get(&wait.resource).copied().flatten(),
                waiting_for: elapsed(observed_at, wait.since),
            })
            .collect::<Vec<_>>();
        drop(state);

        let waits_by_actor = waits
            .iter()
            .map(|wait| (wait.waiter, wait))
            .collect::<BTreeMap<_, _>>();
        let starvations = actors
            .iter()
            .filter_map(|actor| {
                if actor.activity == LivenessActivity::Idle {
                    return None;
                }
                let wait = waits_by_actor.get(&actor.actor).copied();
                let stalled_for = wait.map_or(actor.stalled_for, |wait| wait.waiting_for);
                (stalled_for >= actor.starvation_threshold).then(|| StarvationFinding {
                    actor: actor.actor,
                    domain: actor.domain,
                    job_kind: actor.job_kind,
                    priority: actor.priority,
                    activity: actor.activity,
                    stalled_for,
                    threshold: actor.starvation_threshold,
                    resource: wait.map(|wait| wait.resource.clone()),
                })
            })
            .collect();
        let deadlocks = detect_deadlocks(&waits);

        LivenessSnapshot {
            actors,
            waits,
            starvations,
            deadlocks,
        }
    }

    fn end_wait(&self, actor: LivenessActor, token: u64) {
        let mut state = lock_unpoisoned(&self.inner.state);
        let Some(wait) = state.waits.get(&actor) else {
            return;
        };
        if wait.token != token {
            return;
        }
        let previous_activity = wait.previous_activity;
        state.waits.remove(&actor);
        if let Some(entry) = state.actors.get(&actor) {
            entry.probe.mark_activity(previous_activity);
        }
    }
}

impl Default for LivenessMonitor {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for LivenessMonitor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = lock_unpoisoned(&self.inner.state);
        formatter
            .debug_struct("LivenessMonitor")
            .field("registered_actors", &state.actors.len())
            .field("registered_resources", &state.resources.len())
            .field("active_waits", &state.waits.len())
            .finish()
    }
}

/// Thread-bound lifetime of one registered wait relationship.
///
/// ```compile_fail
/// use superi_concurrency::diagnostics::LivenessWaitGuard;
///
/// fn require_send<T: Send>() {}
/// require_send::<LivenessWaitGuard>();
/// ```
#[must_use = "retain the guard until the represented wait completes"]
pub struct LivenessWaitGuard {
    monitor: LivenessMonitor,
    actor: LivenessActor,
    resource: WaitResource,
    token: u64,
    thread_bound: PhantomData<Rc<()>>,
}

impl LivenessWaitGuard {
    /// Returns the waiting actor.
    #[must_use]
    pub const fn actor(&self) -> LivenessActor {
        self.actor
    }

    /// Returns the resource being awaited.
    #[must_use]
    pub const fn resource(&self) -> &WaitResource {
        &self.resource
    }
}

impl Drop for LivenessWaitGuard {
    fn drop(&mut self) {
        self.monitor.end_wait(self.actor, self.token);
    }
}

impl fmt::Debug for LivenessWaitGuard {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("LivenessWaitGuard")
            .field("actor", &self.actor)
            .field("resource", &self.resource)
            .finish_non_exhaustive()
    }
}

/// One actor's coherent liveness sample.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ActorLivenessSnapshot {
    actor: LivenessActor,
    domain: ExecutionDomain,
    job_kind: Option<JobKind>,
    priority: Option<JobPriority>,
    activity: LivenessActivity,
    progress_revision: u64,
    stalled_for: Duration,
    starvation_threshold: Duration,
}

impl ActorLivenessSnapshot {
    /// Returns the sampled actor.
    #[must_use]
    pub const fn actor(&self) -> LivenessActor {
        self.actor
    }

    /// Returns the sampled actor's execution domain.
    #[must_use]
    pub const fn domain(&self) -> ExecutionDomain {
        self.domain
    }

    /// Returns the job kind when this actor is a scheduled job.
    #[must_use]
    pub const fn job_kind(&self) -> Option<JobKind> {
        self.job_kind
    }

    /// Returns the job priority when this actor is a scheduled job.
    #[must_use]
    pub const fn priority(&self) -> Option<JobPriority> {
        self.priority
    }

    /// Returns the sampled activity.
    #[must_use]
    pub const fn activity(&self) -> LivenessActivity {
        self.activity
    }

    /// Returns the sampled progress revision.
    #[must_use]
    pub const fn progress_revision(&self) -> u64 {
        self.progress_revision
    }

    /// Returns time since the observer last saw activity or progress change.
    #[must_use]
    pub const fn stalled_for(&self) -> Duration {
        self.stalled_for
    }

    /// Returns the configured starvation threshold.
    #[must_use]
    pub const fn starvation_threshold(&self) -> Duration {
        self.starvation_threshold
    }
}

/// One explicit wait and the resource's owner at sample time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WaitSnapshot {
    waiter: LivenessActor,
    resource: WaitResource,
    owner: Option<LivenessActor>,
    waiting_for: Duration,
}

impl WaitSnapshot {
    /// Returns the waiting actor.
    #[must_use]
    pub const fn waiter(&self) -> LivenessActor {
        self.waiter
    }

    /// Returns the awaited resource.
    #[must_use]
    pub const fn resource(&self) -> &WaitResource {
        &self.resource
    }

    /// Returns the current owner, or `None` when the resource is unowned.
    #[must_use]
    pub const fn owner(&self) -> Option<LivenessActor> {
        self.owner
    }

    /// Returns the sampled wait duration.
    #[must_use]
    pub const fn waiting_for(&self) -> Duration {
        self.waiting_for
    }
}

/// One actor whose runnable or waiting state exceeded its threshold.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StarvationFinding {
    actor: LivenessActor,
    domain: ExecutionDomain,
    job_kind: Option<JobKind>,
    priority: Option<JobPriority>,
    activity: LivenessActivity,
    stalled_for: Duration,
    threshold: Duration,
    resource: Option<WaitResource>,
}

impl StarvationFinding {
    /// Returns the starved actor.
    #[must_use]
    pub const fn actor(&self) -> LivenessActor {
        self.actor
    }

    /// Returns the actor's execution domain.
    #[must_use]
    pub const fn domain(&self) -> ExecutionDomain {
        self.domain
    }

    /// Returns the job kind when this actor is a scheduled job.
    #[must_use]
    pub const fn job_kind(&self) -> Option<JobKind> {
        self.job_kind
    }

    /// Returns the job priority when this actor is a scheduled job.
    #[must_use]
    pub const fn priority(&self) -> Option<JobPriority> {
        self.priority
    }

    /// Returns whether the actor was runnable or waiting.
    #[must_use]
    pub const fn activity(&self) -> LivenessActivity {
        self.activity
    }

    /// Returns the no-progress or wait duration.
    #[must_use]
    pub const fn stalled_for(&self) -> Duration {
        self.stalled_for
    }

    /// Returns the configured threshold exceeded by this finding.
    #[must_use]
    pub const fn threshold(&self) -> Duration {
        self.threshold
    }

    /// Returns the blocking resource for a waiting actor.
    #[must_use]
    pub const fn resource(&self) -> Option<&WaitResource> {
        self.resource.as_ref()
    }
}

/// One definite edge in a deadlock cycle.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeadlockEdge {
    waiter: LivenessActor,
    resource: WaitResource,
    owner: LivenessActor,
    waiting_for: Duration,
}

impl DeadlockEdge {
    /// Returns the actor waiting on this edge.
    #[must_use]
    pub const fn waiter(&self) -> LivenessActor {
        self.waiter
    }

    /// Returns the resource being awaited.
    #[must_use]
    pub const fn resource(&self) -> &WaitResource {
        &self.resource
    }

    /// Returns the registered resource owner.
    #[must_use]
    pub const fn owner(&self) -> LivenessActor {
        self.owner
    }

    /// Returns the sampled wait duration.
    #[must_use]
    pub const fn waiting_for(&self) -> Duration {
        self.waiting_for
    }
}

/// One canonical closed cycle in the wait-for graph.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DeadlockFinding {
    edges: Vec<DeadlockEdge>,
}

impl DeadlockFinding {
    /// Returns cycle edges starting at the smallest stable actor identity.
    #[must_use]
    pub fn edges(&self) -> &[DeadlockEdge] {
        &self.edges
    }
}

/// Coherent public liveness evidence from one observer sample.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LivenessSnapshot {
    actors: Vec<ActorLivenessSnapshot>,
    waits: Vec<WaitSnapshot>,
    starvations: Vec<StarvationFinding>,
    deadlocks: Vec<DeadlockFinding>,
}

impl LivenessSnapshot {
    /// Returns actors in stable identity order.
    #[must_use]
    pub fn actors(&self) -> &[ActorLivenessSnapshot] {
        &self.actors
    }

    /// Returns active waits in stable waiter order.
    #[must_use]
    pub fn waits(&self) -> &[WaitSnapshot] {
        &self.waits
    }

    /// Returns starvation findings in stable actor order.
    #[must_use]
    pub fn starvations(&self) -> &[StarvationFinding] {
        &self.starvations
    }

    /// Returns deadlock cycles in stable first-actor order.
    #[must_use]
    pub fn deadlocks(&self) -> &[DeadlockFinding] {
        &self.deadlocks
    }

    /// Projects findings into shared structured diagnostic events.
    pub fn diagnostic_events(&self) -> Result<Vec<DiagnosticEvent>> {
        let mut events = Vec::with_capacity(self.starvations.len() + self.deadlocks.len());
        for finding in &self.starvations {
            let mut event = DiagnosticEvent::new(
                "scheduler.starvation.detected",
                COMPONENT,
                DiagnosticSeverity::Warning,
                "scheduled work exceeded its no-progress threshold",
            )?
            .with_field("actor.id", TraceField::internal(finding.actor.to_string()))?
            .with_field(
                "actor.kind",
                TraceField::internal(finding.actor.kind_code()),
            )?
            .with_field("actor.domain", TraceField::internal(finding.domain.code()))?
            .with_field(
                "actor.activity",
                TraceField::internal(finding.activity.code()),
            )?
            .with_field(
                "stall.nanoseconds",
                TraceField::internal(duration_nanoseconds(finding.stalled_for)),
            )?
            .with_field(
                "threshold.nanoseconds",
                TraceField::internal(duration_nanoseconds(finding.threshold)),
            )?;
            if let Some(kind) = finding.job_kind {
                event.insert_field("job.kind", TraceField::internal(kind.code()))?;
            }
            if let Some(priority) = finding.priority {
                event.insert_field("job.priority", TraceField::internal(priority.code()))?;
            }
            if let Some(resource) = &finding.resource {
                event.insert_field("wait.resource", TraceField::internal(resource.as_str()))?;
            }
            events.push(event);
        }
        for finding in &self.deadlocks {
            let path = finding
                .edges
                .iter()
                .map(|edge| format!("{}>{}>{}", edge.waiter, edge.resource, edge.owner))
                .collect::<Vec<_>>()
                .join("|");
            events.push(
                DiagnosticEvent::new(
                    "scheduler.deadlock.detected",
                    COMPONENT,
                    DiagnosticSeverity::Error,
                    "registered wait ownership contains a closed cycle",
                )?
                .with_field(
                    "cycle.actor_count",
                    TraceField::internal(u64::try_from(finding.edges.len()).unwrap_or(u64::MAX)),
                )?
                .with_field("cycle.path", TraceField::internal(path))?,
            );
        }
        Ok(events)
    }
}

fn detect_deadlocks(waits: &[WaitSnapshot]) -> Vec<DeadlockFinding> {
    let waits_by_actor = waits
        .iter()
        .map(|wait| (wait.waiter, wait))
        .collect::<BTreeMap<_, _>>();
    let mut complete = BTreeSet::new();
    let mut findings = Vec::new();

    for start in waits_by_actor.keys().copied() {
        if complete.contains(&start) {
            continue;
        }
        let mut path = Vec::new();
        let mut positions = BTreeMap::new();
        let mut current = start;
        loop {
            if let Some(position) = positions.get(&current).copied() {
                let mut edges = Vec::with_capacity(path.len() - position);
                for actor in &path[position..] {
                    let Some(wait) = waits_by_actor.get(actor) else {
                        edges.clear();
                        break;
                    };
                    let Some(owner) = wait.owner else {
                        edges.clear();
                        break;
                    };
                    edges.push(DeadlockEdge {
                        waiter: wait.waiter,
                        resource: wait.resource.clone(),
                        owner,
                        waiting_for: wait.waiting_for,
                    });
                }
                if let Some((smallest, _)) =
                    edges.iter().enumerate().min_by_key(|(_, edge)| edge.waiter)
                {
                    edges.rotate_left(smallest);
                    findings.push(DeadlockFinding { edges });
                }
                break;
            }
            if complete.contains(&current) {
                break;
            }
            positions.insert(current, path.len());
            path.push(current);
            let Some(wait) = waits_by_actor.get(&current) else {
                break;
            };
            let Some(owner) = wait.owner else {
                break;
            };
            current = owner;
        }
        complete.extend(path);
    }
    findings.sort_by_key(|finding| finding.edges.first().map(|edge| edge.waiter));
    findings
}

const fn encode_probe(activity: LivenessActivity, revision: u64) -> u64 {
    (revision << ACTIVITY_BITS) | activity as u64
}

const fn decode_probe(encoded: u64) -> ProbeSnapshot {
    ProbeSnapshot {
        activity: LivenessActivity::from_encoded(encoded),
        revision: encoded >> ACTIVITY_BITS,
    }
}

fn elapsed(later: Instant, earlier: Instant) -> Duration {
    later.checked_duration_since(earlier).unwrap_or_default()
}

fn duration_nanoseconds(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

fn is_canonical_name(value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    let mut previous_was_separator = true;
    for byte in value.bytes() {
        match byte {
            b'a'..=b'z' | b'0'..=b'9' => previous_was_separator = false,
            b'.' | b'_' | b'-' if !previous_was_separator => previous_was_separator = true,
            _ => return false,
        }
    }
    !previous_was_separator
}

fn require_actor<'a>(
    state: &'a MonitorState,
    actor: LivenessActor,
    operation: &'static str,
) -> Result<&'a ActorEntry> {
    state.actors.get(&actor).ok_or_else(|| {
        diagnostic_error(
            ErrorCategory::NotFound,
            Recoverability::UserCorrectable,
            "liveness actor is not registered",
            operation,
        )
        .with_context(actor_context(actor))
    })
}

fn actor_context(actor: LivenessActor) -> ErrorContext {
    ErrorContext::new(COMPONENT, "actor")
        .with_field("actor", actor.to_string())
        .with_field("actor_kind", actor.kind_code())
}

fn resource_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    message: &'static str,
    operation: &'static str,
    resource: &WaitResource,
) -> Error {
    diagnostic_error(category, recoverability, message, operation).with_context(
        ErrorContext::new(COMPONENT, "resource")
            .with_field("resource", resource.as_str().to_owned()),
    )
}

fn diagnostic_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    message: &'static str,
    operation: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}

fn lock_unpoisoned<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
