---
module_id: superi-concurrency
source_paths:
  - open/crates/superi-concurrency
source_hash: 9ce012335b1e1f30f8fd86ab8813f68f90a8c6eb1878cc28ddded4a4fd634310
source_files: 22
mapped_at_commit: working-tree
---

## Purpose and ownership

`superi-concurrency` is the T1 systems crate that defines Superi's execution ownership, bounded
scheduling, cross-thread publication, playback timing, lifecycle coordination, and liveness
contracts. It sits below graph, audio, and engine orchestration. The crate owns reusable mechanisms
and stable diagnostic identities, but it does not own media pipeline orchestration, application
policy, concrete subsystem event loops, audio buffers, GPU command encoding, or public API
projection.

Implemented responsibilities are:

- Seven explicit execution domains with platform-owned versus Superi-owned thread policy.
- Deterministic weighted FIFO job scheduling, bounded background execution, local queue ownership,
  work stealing, cooperative cancellation and deadlines, progress, dependency observations, and
  typed terminal outcomes.
- Anchor-based monotonic and audio-master playback clocks plus playback-owned A/V frame decisions.
- Fixed-capacity nonblocking handoffs that retain a saturated payload for producer-controlled retry.
- Single-domain mutable ownership and immutable, generation-tagged snapshot publication.
- Revisioned, acknowledged startup, pause, resume, sleep, wake, failure, shutdown, and restart
  coordination.
- Lock-free actor progress publication with observer-side starvation and wait-for-cycle analysis.

GPU submission is only represented as an execution domain and a contract test against
`superi-gpu`. The dedicated submission module remains a skeleton. The crate exposes building
blocks, not a composed runtime: audio and engine construct selected domain, clock, and worker
mechanisms, but no owner wires the complete state machines together.

## Source inventory

- `open/crates/superi-concurrency/Cargo.toml` declares the crate, workspace policy, production
  dependencies on `crossbeam-queue`, `superi-core`, and `superi-gpu`, and test dependencies on
  `pollster` and `superi-media-io`.
- `open/crates/superi-concurrency/src/lib.rs` is the public crate entry point and exports
  `backpressure`, `clock`, `diagnostics`, `jobs`, `lifecycle`, `shared`, `submit`, and `threads`.
- `open/crates/superi-concurrency/src/backpressure.rs` implements typed pipeline-stage routes,
  positive capacity validation, preallocated `ArrayQueue` handoffs, nonblocking send and receive,
  saturation ownership recovery, and occupancy snapshots.
- `open/crates/superi-concurrency/src/clock.rs` implements monotonic playback clocks, audio-master
  sample publication, exact A/V drift measurement, scheduling policy, frame actions, and scheduler
  statistics.
- `open/crates/superi-concurrency/src/diagnostics.rs` implements liveness actors, lock-free probes,
  registered wait resources, thread-bound wait guards, starvation sampling, canonical deadlock
  findings, and structured diagnostic-event projection.
- `open/crates/superi-concurrency/src/jobs.rs` defines cancellation, deadlines, progress,
  dependency declarations and observations, terminal outcomes, and completion reports, and
  reexports the priority and worker-pool surfaces.
- `open/crates/superi-concurrency/src/jobs/pool.rs` implements bounded background workers,
  round-robin local queue assignment, global weighted dispatch, work stealing, typed one-shot
  completion handles, panic containment, snapshots, draining shutdown, and worker joining.
- `open/crates/superi-concurrency/src/jobs/priority.rs` defines job kinds, four priority classes,
  the 8:4:2:1 service pattern, derived-media fallback selection, scheduled envelopes, and the
  synchronous priority scheduler.
- `open/crates/superi-concurrency/src/lifecycle.rs` implements fixed participant registration,
  atomic phase and revision signals, acknowledged transitions, retained failures, blocking-safe
  change observation, and restart after teardown.
- `open/crates/superi-concurrency/src/shared.rs` implements non-`Sync` domain-owned mutable values,
  single-owner snapshot publishers, monotonic snapshot generations, and immutable shared snapshot
  handles.
- `open/crates/superi-concurrency/src/submit.rs` is a three-line placeholder for the GPU command
  submission and synchronization model and contains no types or behavior.
- `open/crates/superi-concurrency/src/threads.rs` implements execution-domain identities and
  policies, thread-local ownership, scoped platform-domain entry, named managed-thread spawning,
  domain checks, and typed join handles.
- `open/crates/superi-concurrency/tests/av_sync_contract.rs` verifies signed drift, playback-domain
  enforcement, wait, present, correction, protected-drop, starvation, discontinuity, validation,
  and statistics behavior.
- `open/crates/superi-concurrency/tests/backpressure_contract.rs` verifies route and bound
  validation, exact video and audio payload preservation, independent route capacity, snapshots,
  concurrent MPMC delivery, and endpoint auto traits.
- `open/crates/superi-concurrency/tests/execution_domains_contract.rs` verifies stable domains and
  policies, thread-local entry, managed names and failures, concurrent domain isolation, and a real
  `GpuSubmissionQueue` hosted by the GPU submission domain when a GPU adapter is available.
- `open/crates/superi-concurrency/tests/job_lifecycle_contract.rs` verifies cooperative
  interruption, deterministic dependency resolution, contended progress, typed completion detail,
  and shared error and auto-trait contracts.
- `open/crates/superi-concurrency/tests/job_priority_contract.rs` verifies stable scheduling codes,
  FIFO identity, weighted service, empty-class skipping, queued identity reuse, and deterministic
  derived-media selection.
- `open/crates/superi-concurrency/tests/lifecycle_contract.rs` verifies phase identity, immediate
  empty-participant transitions, acknowledgements, stale revisions, sleep and wake intent, retained
  failures, restart, atomic observation, and blocking-domain restrictions.
- `open/crates/superi-concurrency/tests/liveness_diagnostics_contract.rs` verifies actor and
  resource validation, atomic progress under contention, starvation thresholds, RAII wait cleanup,
  owner transfer, canonical cycles, event projection, and public auto traits.
- `open/crates/superi-concurrency/tests/playback_clock_contract.rs` verifies stable clock modes,
  nonaccumulating monotonic reads, exact audio sample timing, rejected resets, mode continuity,
  reanchoring, and cross-thread transfer.
- `open/crates/superi-concurrency/tests/shared_state_contract.rs` verifies domain-enforced mutable
  ownership, cross-thread immutable scheduling snapshots, audio publication rejection, pointer
  identity, and generation exhaustion.
- `open/crates/superi-concurrency/tests/worker_pool_contract.rs` verifies worker and queue bounds,
  active parallelism, stealing, global priority service, cooperative lifecycle outcomes, panic
  containment, nonblocking observation, wait restrictions, and pool auto traits.

## Public surface

The crate root exposes eight named modules. Fields are generally private and observed through
typed accessors. Public enums that cross subsystem or diagnostic boundaries are `#[non_exhaustive]`
where downstream matching must remain forward compatible.

### Execution domains and threads

- `ExecutionDomain` names `Ui`, `EngineControl`, `Playback`, `Render`, `Audio`, `BackgroundJob`,
  and `GpuSubmission`, publishes stable codes and names, and returns an `ExecutionPolicy`.
- `ExecutionPolicy` reports platform ownership, blocking permission, and allocation permission.
  UI and audio are platform-owned. UI, engine control, playback, and audio disallow ordinary
  blocking. Audio alone disallows allocation and freeing.
- `ExecutionDomain::enter_current` installs one thread-local domain until the returned
  `ExecutionDomainGuard` drops. `require_current` validates callers, and
  `current_execution_domain` observes the current assignment.
- `ExecutionDomain::spawn` creates one named managed thread for non-platform domains and returns an
  `ExecutionDomainThread<T>`. The handle exposes domain, name, thread identity, and a joining
  operation that preserves structured worker errors and classifies panics.

### Job scheduling and execution

- `JobKind`, `JobPriority`, `DerivedQuality`, `DerivedFallbackPolicy`, and
  `DerivedSelectionReason` expose stable codes. Priorities are background, export, playback, and
  interactive with saturated service weights 1, 2, 4, and 8.
- `DerivedMediaCandidate`, `DerivedMediaRequest`, and `DerivedMediaSelection` preserve
  authoritative `MediaId` plus source revision while selecting an exact or nearest lower fresh
  `CacheId`, with lowest cache identity resolving equal-quality ties.
- `ScheduledJob<T>` carries `JobId`, kind, priority, optional derived-media intent, and an opaque
  payload. `PriorityScheduler<T>` owns one FIFO per class, rejects duplicate queued identities,
  advances a fixed 15-slot weighted cursor, and skips empty classes.
- `JobCancellationToken` is a cloneable one-way atomic flag. `JobControl` combines that flag with
  an optional process-local `Instant` deadline, gives cancellation precedence over simultaneous
  expiry, and returns classified errors from explicit checkpoints.
- `JobProgress` and `JobProgressSnapshot` provide mutex-protected coherent progress with an
  optional positive total, monotonic completed units, and a revision that changes only on state
  change.
- `JobDependencies` validates and sorts direct prerequisites. `JobDependencyTracker` records each
  prerequisite's immutable terminal state and reports `Ready`, ordered `Waiting`, or ordered
  `Blocked` state. `JobDependencyFailure` names one non-successful prerequisite.
- `JobTerminalState`, `JobOutcome<T>`, and `JobCompletion<T>` retain stable terminal
  classification, a successful typed value or structured failure detail, final progress, identity,
  and elapsed execution time.
- `WorkerPoolConfig` bounds workers to 1 through 256 and gives the pending queue a positive hard
  capacity. `recommended` samples available parallelism once and reserves two logical slots when
  possible.
- `BoundedWorkerPool` starts fixed `BackgroundJob` workers, submits typed closures without waiting
  for queue capacity, exposes a coherent `WorkerPoolSnapshot`, drains and joins on `shutdown`, and
  also drains and joins from `Drop`.
- `JobExecutionContext` exposes operation identity, executing and preferred worker indices,
  `DispatchOrigin`, controls, progress, and a cooperative `check` method. `JobTaskHandle<T>` offers
  nonblocking `try_completion` and blocking-safe `wait`; `JobExecution<T>` adds dispatch ownership
  to the typed completion.

### Playback clocks and A/V scheduling

- `AudioMasterClock` retains one fixed sample rate and one monotonically nondecreasing atomic
  sample position. `publish` rejects a frequency change and `publish_sample` rejects regression;
  successful publication returns `AudioClockUpdate` without locking or allocation.
- `PlaybackClock` maps a timeline anchor to either a monotonic `Instant` anchor or an
  `AudioMasterClock` sample anchor. It supports checked reads, exact audio-master timing, mode
  switches that preserve timeline continuity, and explicit reanchoring.
- `measure_av_drift` is a pure checked conversion of video PTS minus master time into signed
  nanoseconds and returns `AvDriftMeasurement` plus `AvDriftDirection`.
- `AvSyncPolicy` validates tolerance, drop, discontinuity, correction, and consecutive-drop bounds.
  `AvFrameObservation` carries PTS, master time, frame duration, successor readiness, and drop
  eligibility.
- `AvSyncScheduler` requires the playback domain and returns an `AvScheduleDecision` containing
  exact drift and an `AvFrameAction`: bounded `Wait`, interval-adjusted `Present`, `DropLate`, or
  caller-applied `Rebase`. `AvSyncStatistics` reports saturating counters and drop streak state.

### Handoffs and shared state

- `PipelineStage`, `PipelineRoute`, and `BackpressureConfig` identify a directed route between
  distinct semantic media owners and require a positive independent item capacity.
- `bounded_handoff<T>` returns cloneable `HandoffSender<T>` and `HandoffReceiver<T>` endpoints over
  a preallocated `ArrayQueue`. `try_send` either enqueues or returns `Backpressure<T>` containing
  the exact payload. `try_receive` returns immediately, and `HandoffSnapshot` exposes point-in-time
  occupancy without reserving capacity.
- `DomainOwned<T>` is `Send` when its payload is `Send` but deliberately not `Sync`. Its read,
  mutation, and consuming operations run inline only in the configured execution domain.
- `SnapshotPublisher<T>` is a non-`Sync` single-owner publisher. It validates its domain, rejects
  allocation-free domains, advances `SnapshotGeneration` only after validation, and clones an
  existing `Arc<T>` into `SharedSnapshot<T>`. Snapshots retain source domain, generation, pointer
  identity, and immutable payload access.

### Lifecycle and diagnostics

- `LifecyclePhase` supplies stable phase codes and identifies the six transitional phases that
  require acknowledgement. `LifecycleSignal` publishes phase and revision in one atomic word, and
  `LifecycleSignalSnapshot` is the exact acknowledgement token.
- `LifecycleCoordinator` validates a fixed participant set, returns `LifecycleParticipant`
  handles, exposes lock-free signals and coherent snapshots, requests transitions without waiting,
  and offers blocking-safe `wait_for_change`. `LifecycleFailure` retains participant identity and
  the original shared error.
- `LivenessActor` identifies a domain or `JobId`. `LivenessRegistration` attaches domain, optional
  job metadata, and a positive starvation threshold. `LivenessProbe` publishes idle, runnable,
  waiting, and monotonic progress state through one atomic word.
- `WaitResource` validates a canonical lowercase resource identity. `LivenessMonitor` registers
  actors and single resource owners, begins one active wait per actor, transfers or releases
  ownership, and samples state. `LivenessWaitGuard` is thread-bound and removes its exact tokenized
  wait on drop.
- `LivenessSnapshot` exposes actor samples, waits, `StarvationFinding` values, and canonical
  `DeadlockFinding` cycles with explicit `DeadlockEdge` ownership. `diagnostic_events` projects
  findings into shared warning and error events.
- `submit` currently exposes no public type or function.

## Architecture and data flow

### Thread roles

The UI and audio domains attach platform-owned threads. Engine control is the intended serialized
mutable-state owner. Playback owns the master clock and A/V scheduler. Render may block while
coordinating immutable snapshots. Background workers execute scheduled closures. GPU submission is
the sole intended queue-submission and retirement owner. Thread-local entry prevents one thread
from holding two domains simultaneously, while separate managed threads allow those responsibilities
to progress independently.

The domain abstraction establishes ownership but does not create subsystem loops. A managed spawn
runs one closure under a guard and exits when the closure returns. The worker pool creates its own
named `superi-background-job-N` threads and enters `BackgroundJob` directly rather than using one
`ExecutionDomainThread` per long-lived worker.

### Scheduling, queues, identity, and completion

1. A producer constructs a `ScheduledJob` with a stable `JobId`, kind, explicit priority, optional
   derived-media request, and closure payload.
2. `BoundedWorkerPool::submit` erases the closure behind an internal task, creates a capacity-one
   result channel, and assigns the envelope to one local priority queue in round-robin order.
3. Queue admission holds one pool mutex. It rejects shutdown, then full pending capacity, then a
   duplicate identity currently in the pending index. Active jobs do not count toward pending
   capacity or identity uniqueness.
4. Every waiting worker contends on the same pool state and condition variable. Under the mutex, a
   single global 15-slot cursor chooses priority first. The worker checks its own local FIFO, then
   victim queues in increasing circular distance and records owned or stolen dispatch.
5. The pending identity is removed and active count incremented before user code runs. The worker
   releases the pool mutex, creates `JobExecutionContext`, checks cancellation or deadline once at
   job start, and calls the closure.
6. The closure must call `check` between bounded work units. A success becomes `Succeeded`; shared
   cancellation and timeout errors become their matching terminal outcomes; other errors retain
   detail as `Failed`; a panic is contained and classified as terminal internal failure.
7. The worker snapshots progress, sends one `JobExecution` to the handle, then records completion
   under the pool mutex. A disconnected observer does not fail the worker.
8. Shutdown stops admission, wakes workers, drains queued jobs, and joins all workers. A worker
   exits only after no queued task remains and admission is closed.

The standalone `PriorityScheduler` uses the same service pattern without execution. Its fixed
pattern guarantees at most 14, 7, 3, and 1 intervening dispatches for continuously queued
background, export, playback, and interactive work under saturated single-owner dispatch. FIFO
order remains stable within each class.

`JobDependencies` and `JobDependencyTracker` are adjacent lifecycle primitives, not integrated
into queue admission or dispatch. Callers build the graph, detect multi-job cycles, hold dependent
jobs, record prerequisite results, and construct any `DependencyFailed` outcome themselves.

### Cancellation, deadlines, and backpressure

Cancellation is one-way and shared through an `Arc<AtomicBool>`. Deadlines are monotonic
process-local instants. Both are cooperative observations, not preemption. The worker pool checks
before invoking a closure, then the closure controls further responsiveness through bounded
checkpoints.

Worker-pool saturation is reported as a retryable structured error without blocking, but submission
takes the scheduled envelope by value and does not return its closure to the caller. Pipeline
handoffs use a different typed flow-control result: a full route returns the exact `T` in
`Backpressure<T>`. Each route has independent capacity, so an export queue cannot consume audio or
viewport slots. The handoff layer never chooses whether a video frame may be dropped and never
copies or interprets media metadata.

### Mutable ownership and immutable publication

`DomainOwned<T>` moves mutable state into one domain and verifies thread-local ownership for every
access. `SnapshotPublisher<T>` separately turns a caller-prepared `Arc<T>` into immutable snapshots
with monotonically increasing generations. Publication advances the generation only after the
domain and allocation-policy checks pass. Readers may clone the `Arc` across threads while old
snapshots remain stable. This is suitable for UI, render, and scheduling observations, but not for
the real-time audio callback because cloning or final dropping reference-counted ownership may free
memory.

### Playback timing and A/V decisions

In monotonic mode, `PlaybackClock` recomputes timeline position from a fixed timeline and `Instant`
anchor on every read and rounds once to the timeline timebase. In audio-master mode, the callback
publishes a fixed-rate sample counter and playback computes exact elapsed samples from a fixed
sample anchor. Reads never integrate a previously rounded result, which prevents polling-frequency
drift.

The playback owner supplies a frame observation to `AvSyncScheduler`. It first validates domain and
positive frame duration, measures drift, and then chooses in order: rebase for a discontinuity,
wait for video beyond the early tolerance, present inside tolerance, correct moderate lateness,
protect a non-droppable or last-ready frame, force progress at the drop-streak cap, or drop an
eligible late frame. The result is an instruction only. The caller performs pacing, presentation,
frame disposal, and clock reanchoring.

### Lifecycle and liveness

Lifecycle begins at revision zero in `Starting` for a nonempty participant set or `Running` for an
empty set. A request advances the revision, publishes a transitional phase, copies all fixed
participants into the pending set, and returns immediately. Participants observe the atomic token,
finish their phase work, and acknowledge that exact phase and revision. The last acknowledgement
advances the revision again and publishes the stable phase. A newer request invalidates old tokens.
Failure clears pending work, retains the participant and structured error, and enters `Failed`.
Only orderly shutdown to `Stopped` permits a restart that clears failure state.

Liveness is opt-in and separate from lifecycle and worker scheduling. Actors publish activity and
progress without taking the monitor mutex. The observer samples those atomics under its registry
mutex, compares them with the last observation time, snapshots explicit waits and resource owners,
then analyzes findings after releasing the lock. Runnable or waiting actors at or beyond their
threshold are starved. A deadlock requires every edge in a closed actor-to-owner path to have a
registered resource owner; unowned and incomplete paths remain visible waits but are not classified
as cycles.

### Error paths

All fallible public operations use `superi_core::error::Error` with category, recoverability, and
component-specific context. Invalid configuration and domain misuse are generally user-correctable.
Capacity exhaustion and transient clock regressions are retryable. Cancellation and timeout retain
their own categories. Revision or generation exhaustion, lost result channels, worker panics, and
invalid internal states are terminal. Poisoned internal mutexes are recovered because state changes
validate before mutation and do not invoke user code while locked; user closures run outside pool
and progress locks.

## Dependencies and consumers

Production dependencies and internal relationships are:

- `superi-core` supplies the shared error model, `JobId`, `MediaId`, `CacheId`, exact rational and
  sample time types, pixel and color contracts used by tests, and structured diagnostics.
- `crossbeam-queue` supplies the fixed-capacity MPMC `ArrayQueue` used only by backpressure
  handoffs.
- `superi-gpu` is declared as a normal dependency, but current production source does not import
  it. The execution-domain contract test uses its real device and submission queue to prove that
  GPU ownership can be hosted in `GpuSubmission`.
- The standard library supplies threads, thread-local storage, atomics, `Arc`, `Mutex`, `Condvar`,
  MPSC result channels, monotonic `Instant`, ordered maps and sets, and panic containment.
- Internal dependency direction is `threads` into `shared`, `clock`, `lifecycle`, and `jobs::pool`;
  `jobs::priority` into `jobs::pool`; and `threads` plus job metadata into `diagnostics`.
  Backpressure is otherwise independent.
- `pollster` and `superi-media-io` are development-only dependencies. The former drives async GPU
  setup in a synchronous contract test. The latter supplies real video and audio payloads for
  handoff preservation tests.

Direct workspace consumers declared in manifests are `superi-graph`, `superi-audio`,
`superi-cache`, and `superi-engine`. `superi-graph` does not import runtime surfaces in production source.
`superi-audio::graph` enforces `ExecutionDomain::Audio` at its prepared processing boundary.
`superi-audio::sync` enforces it for exact callback scheduling and audio-master publication, while
`superi-audio::playback` enters it inside the production output callback and advances the same clock.
`superi-cache::render` owns a `BoundedWorkerPool`, submits exact `JobKind::Cache` work at
`JobPriority::Background` with `JobControl`, exposes typed task completion and worker snapshots,
and composes queue-local exact-frame single-flight over the generic pool.
`superi-engine::proxy_substitution` consumes
`DerivedQuality`, `DerivedFallbackPolicy`, `DerivedMediaCandidate`, `DerivedMediaRequest`, and
`DerivedMediaSelection` without constructing the runtime. `superi-engine::playback` enforces the
playback domain,
submits playback-priority cache closures to `BoundedWorkerPool`, cooperatively cancels superseded
generations, advances determinate progress, and polls `JobTaskHandle` without blocking. Its
controller stores only a weak pool reference so a blocking-safe lifecycle owner remains responsible
for shutdown. Foreground playback also submits one exact `JobKind::Frame`, paces its completed value
through engine-owned `AvSyncCoordinator`, which composes `PlaybackClock` and `AvSyncScheduler`,
applies requested rebases to the concrete clock, exposes exact drift and statistics, and publishes
through `HandoffSender` while retaining the payload and resolved presentation returned by
backpressure. `superi-engine::transport` keeps all mutations on the playback domain, reanchors that
coordinator at explicit discontinuities, assigns distinct frame deadlines and live presentation
durations in the clock timebase, cancels stale frame and prediction tokens, and exposes bounded
handoff pressure without waiting. `superi-engine::lifecycle` composes
`LifecycleCoordinator` as one engine participant, keeps authoritative state in `DomainOwned` on
EngineControl, publishes every committed transition through `SnapshotPublisher`, and exposes the
coordinator's lock-free `LifecycleSignal` to latency-sensitive consumers. Engine render-export is
the first concrete workflow to require the resulting revision-scoped export permit both before
codec creation and immediately before artifact publication, so rendering or export degradation and
recovery cannot be bypassed by a long-running transaction. `superi-engine::export_jobs` wraps that
transaction in a bounded logical queue: it submits `JobKind::Export` work at
`JobPriority::Export`, retains reconstructible executors across temporary saturation, combines
`JobControl` with an export-priority media operation context, reports `JobProgress`, polls typed
completion without waiting, and composes `JobDependencyTracker` into deterministic dependency
admission and terminal propagation. The broader downstream
crate graph is not evidence that other transitive crates call these runtime surfaces.
The concurrency integration-test files, cache render contract, audio graph contract, engine
substitution contract, engine playback and transport contracts, engine export queue and render-export
contracts, and engine lifecycle contract exercise the public crate paths directly. No public API crate currently
reexports these types. Engine composes lifecycle with export admission, while foreground playback
and transport consume A/V drift decisions with the actual audio clock, graph result, and viewport
handoff. Lifecycle does not yet own that foreground flow, and no end-to-end runtime composes it with
liveness or native GPU submission.

## Invariants and operational boundaries

- A thread has at most one entered execution domain. Dropping its thread-bound guard clears the
  assignment. Platform-owned UI and audio domains cannot be spawned by the crate.
- UI, engine-control, playback, and audio code must not use blocking pool or lifecycle waits.
  `JobTaskHandle::wait` and `LifecycleCoordinator::wait_for_change` enforce that policy at runtime.
  Managed-thread `join`, pool `shutdown`, and pool `Drop` document the same restriction but do not
  inspect the caller's current domain.
- The audio callback may load lifecycle and liveness atomics, publish an audio sample counter, and
  use already-created scalar handoffs. It must not publish or finally drop reference-counted shared
  snapshots, block, or allocate.
- Worker count and pending queue capacity are explicit hard bounds. Active tasks are bounded by
  worker count but do not consume pending capacity. Queue storage for handoffs is fixed at creation.
- The weighted service cursor is shared across local worker queues, and priority is chosen before
  ownership or stealing. Timing can change which worker acquires the pool mutex and which local
  queue becomes the victim, but not the next nonempty priority selected from the global cursor.
- `JobId` uniqueness is enforced only while a job remains queued. Dispatch removes the identity,
  so the same ID can be submitted again while an earlier lifetime is active. Consumers must treat
  reuse as a deliberate new lifecycle and must not assume process-wide or in-flight uniqueness.
  The engine export queue closes this generic gap for retained logical export jobs by reserving each
  identity across every attempt until explicit safe removal.
- Cancellation is idempotent, irreversible, and cooperative. It cannot interrupt an unbounded
  foreign call, blocked user closure, or work that omits checkpoints. Deadlines use process-local
  monotonic time and are not persistent timestamps.
- Job progress never decreases or exceeds a known total. A state-changing update increments the
  revision exactly once; idempotent updates do not. Direct dependency identities are sorted and
  unique, and a recorded terminal result cannot be replaced.
- Derived media never replaces authoritative source identity. Only candidates with the exact
  source and revision are eligible. Fallback can choose exact quality, the closest lower quality,
  or source, never a higher quality.
- Backpressure never drops, replaces, copies, or semantically interprets a saturated payload. It
  returns ownership to the producer. Occupancy snapshots are observations, not reservations.
- Mutable shared state has one explicit domain owner. Published snapshots are immutable and their
  generations increase by one without reuse. Exhaustion fails before publication.
- Audio-master sample rate is immutable for a source and sample position never decreases. A reset
  or device-format change requires a new source and explicit clock reanchor. Playback position is
  always recomputed from anchors rather than integrated from prior reads.
- A/V scheduling mutates statistics only after domain and observation validation. It never sleeps,
  submits, mutates the audio clock, or performs the returned action. Discontinuity and starvation
  policies bound drop behavior.
- Lifecycle participants are fixed and deterministically ordered. Every transitional revision
  requires one acknowledgement from each participant. A stale token cannot complete a newer
  transition, and restart is legal only from `Stopped`.
- Liveness findings depend only on explicitly registered actors, resources, owners, waits, and
  progress. Idle actors are never starved. Wait guards are thread-bound, tokenized, and restore the
  activity observed at wait start when dropped. Deadlock reports contain only closed owned cycles.
- Internal mutex poison is recovered rather than propagated. User code is not executed while the
  pool, progress, dependency, lifecycle, or liveness registry mutex is held.

## Tests and verification

The crate has ten public integration suites plus compile-fail documentation examples for
non-`Send` or non-`Sync` guards and owners. Test coverage is contract-oriented:

- Execution-domain tests prove stable identity, policy, isolation, named threads, failure
  classification, and real GPU queue hosting. The GPU case prints a skip and returns successfully
  when no adapter is available, so it is environment-conditional rather than mandatory hardware
  proof.
- Priority and worker-pool tests prove the 15-slot service pattern, FIFO behavior, capacity and
  active bounds, stealing, background-domain entry, typed results, cancellation before execution,
  progress, panic containment, and wait restrictions.
- Job-lifecycle tests prove cooperative cancellation and deadlines, sorted dependency outcomes,
  contended progress, terminal classification, and structured error retention.
- Engine export queue tests prove the generic pool, progress, control, completion, and dependency
  surfaces can form a bounded nonblocking logical scheduler with fresh recovery attempts and retained
  results. The real render-export consumer additionally proves semantic progress reaches publication.
- Clock and A/V tests prove exact sample timing, checked monotonic anchoring, mode continuity,
  signed cross-timebase drift, playback ownership, bounded correction, conservative dropping,
  forced visible progress, and resettable statistics.
- Engine coordination tests additionally prove a concrete caller applies one rebase, preserves
  immutable frame timing, exposes correction and recovery evidence, and does not reschedule a
  resolved presentation after viewport backpressure.
- Backpressure tests send real `VideoFrame` and `AudioBlock` values to prove metadata, timing,
  format, channel ordering, and shared-buffer identity survive. Concurrent producers prove each
  scalar payload is delivered once while respecting a hard capacity.
- Shared-state tests prove domain ownership, immutable cross-thread snapshots, pointer identity,
  audio rejection, and generation exhaustion.
- Lifecycle tests prove exact revisions, idempotent requests, stale-acknowledgement rejection,
  pause intent across sleep, retained failure, restart, atomic observation, and change-wait policy.
  Engine lifecycle tests additionally prove one EngineControl owner composes the coordinator,
  `DomainOwned`, `SnapshotPublisher`, and the lock-free signal for deterministic subsystem startup,
  degraded recovery, reverse teardown, and restart.
- Liveness tests prove canonical validation, atomic progress under contention, exact starvation
  thresholds, RAII wait removal, unowned-resource handling, one- and two-actor cycles, stable event
  projection, and public auto traits.

There is no test for `submit` because it has no behavior. The engine foreground playback contract
wires cancellable jobs, audio-master and monotonic clock modes, A/V drift decisions and diagnostics,
and a bounded handoff around a real graph frame. The transport contract adds exact cross-timebase
frame deadlines, rate-adjusted live durations, bounded prediction, discontinuity cancellation, and
lossless handoff across real control transitions. Together they prove bounded wait and correction,
applied discontinuity recovery, and exact transport pacing, but no end-to-end consumer yet adds
lifecycle or native GPU submission. Contract suites validate each mechanism and selected production
combinations, not the future complete engine runtime.

## Current status and risks

Most focused mechanisms are substantive and strongly contract-tested, but integration remains
incomplete:

- `submit` is an explicit placeholder. The crate-level description promises GPU coordination, but
  current implementation stops at domain ownership and a test-hosted `superi-gpu` queue.
- Cache now uses worker scheduling, typed completion, cancellation, deadlines, and progress for
  bounded background rendering. Engine playback uses domain ownership, worker scheduling,
  cancellation, progress, and nonblocking result polling for predictive cache work. Its foreground
  path adds cancellable exact frame jobs, audio-master pacing with monotonic fallback, and lossless
  viewport backpressure. Engine transport now composes those mechanisms into exact seek, scrub,
  step, signed-rate, direction, loop, and bounded late-frame control, while engine proxy resolution
  uses the derived-media selector. Audio uses the audio
  execution domain for prepared processing, exact scheduling, and device output and publishes the
  completed presentation clock. Engine also composes lifecycle acknowledgement and shared snapshot
  publication as a control plane, but it does not drive the prepared resource or foreground
  playback owners through those actions. Foreground engine playback now composes A/V hold,
  correction, eligible dropping, starvation protection, and rebase decisions with its actual clock
  and viewport. Decoded-sample scheduling, prepared audio graph execution, GPU submission, and
  liveness are not yet composed into one application flow. Render-export enforces its current permit
  at transaction admission and publication, but lifecycle still does not acquire codecs or drive the
  prepared resource and foreground playback owners through subsystem actions.
- Dependency tracking is passive. The worker pool does not delay a job until prerequisites are
  ready, propagate terminal states, create `DependencyFailed`, or detect cycles across submitted
  jobs. The engine export queue now provides that orchestration for its own retained logical jobs by
  admitting only dependencies that already exist, polling dependency history before dispatch, and
  finalizing terminal downstream outcomes. The generic pool remains passive for other consumers.
- Operation identity is not globally unique. Removing `JobId` from the pending index at dispatch
  permits overlapping active lifetimes with the same identifier, which can make cancellation,
  dependency, result, or diagnostic correlation ambiguous if a consumer assumes one live
  operation per ID. The engine export queue reserves identity for the full retained logical lifetime,
  but the generic worker API still permits reuse.
- Cancellation and deadline enforcement is only as responsive as user checkpoints. Shutdown and
  `Drop` drain and join, so either can block indefinitely on a closure that waits forever or omits
  cooperative termination.
- Blocking restrictions are enforced for task-result and lifecycle waits but not for
  `ExecutionDomainThread::join`, `BoundedWorkerPool::shutdown`, or pool destruction. Calling those
  from UI, engine control, playback, or audio can violate the documented latency boundary.
- Pool priority selection follows one global cursor, but exact worker ownership and interleaving
  remain timing-dependent. Saturated starvation bounds describe dispatch counts, not wall-clock
  latency, and a long-running or blocked closure can still delay useful capacity.
- A full worker queue returns a retryable error but not the submitted scheduled envelope. Callers
  that need lossless retry must retain reconstructible operation state outside the moved closure.
- Backpressure handoffs provide no blocking wait, async notification, endpoint-closure state,
  timeout, or fairness guarantee. Producers and consumers must arrange retry and pacing without a
  busy loop.
- Lifecycle has fixed participants and no built-in acknowledgement timeout, forced teardown,
  participant removal, or rollback. One failed or stalled participant can hold a transition until
  an external owner reports failure or requests another transition.
- Liveness is manual instrumentation and is not wired into worker or lifecycle state. Missing
  progress marks, waits, resources, or owners cause false negatives; inaccurate registration can
  produce misleading starvation or cycle findings. Findings are diagnostic observations, not
  recovery actions.
- A/V `Wait` and `Rebase` remain caller-applied instructions at this reusable boundary. The engine
  foreground coordinator now applies rebases and retains resolved presentations correctly, but
  another caller can still report decisions without executing them and this module cannot guarantee
  every consumer's clock mutation or eventual presentation.
- Snapshot publication clones `Arc` ownership and is intentionally unsuitable for hard real-time
  payload publication. Consumers must retain preallocated atomic or buffer paths for audio.
- `superi-gpu` is a normal dependency despite being unused by production code in this crate. Until
  submission behavior is implemented, it increases the crate dependency surface only for the
  integration contract.

## Maintenance notes

- Run `python3 .agents/skills/superi-mapping/scripts/codebase_maps.py files superi-concurrency` and
  `python3 .agents/skills/superi-mapping/scripts/codebase_maps.py hash superi-concurrency` after any
  source, manifest, or test change, then update both inventory prose and frontmatter together.
- Preserve stable code strings, domain policies, priority weights, lifecycle phase discriminants,
  atomic packing widths, and structured error contexts unless a deliberate public contract change
  updates all callers and tests.
- When `submit` becomes substantive, map the real command ownership, queue admission, fence and
  retirement flow, device-loss recovery, and its relationship to `ExecutionDomain::GpuSubmission`.
- When graph, engine, API, or CLI begins using more of these surfaces, replace the declared-consumer
  description with the exact construction and event paths and update the relevant consumer maps.
  Keep the engine's derived selection, playback worker, A/V coordinator, transport scheduler, and
  lifecycle-control uses distinct from a complete runtime composition. Transport must keep clock
  mapping anchor based, cancellation cooperative, and viewport backpressure lossless.
  Keep the existing audio scheduler, device callback, and audio-master publication paths current as
  engine integration expands.
- If job dependencies become scheduler-owned, document admission, cycle detection, cancellation
  propagation, terminal result publication, identity lifetime, and how they interact with weighted
  queues and shutdown. The engine export queue is the first consumer-owned composition and must keep
  its older-only admission, retryable unresolved state, terminal propagation, retained identity, and
  blocking-safe shutdown rules explicit.
- Keep latency-sensitive claims honest. Audit allocation, final-drop, locking, and blocking behavior
  when changing atomic signals, handoffs, audio clocks, snapshots, worker handles, or lifecycle
  observers.
- Add composed-runtime tests when an engine consumer exists. The current focused suites should
  remain as low-level contract proof rather than being replaced by broader integration coverage.
