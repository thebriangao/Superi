//! `superi-concurrency`, execution domains, deterministic job scheduling, clocks, and GPU
//! coordination.
//!
//! The [`threads`] module enforces the Phase 0 ownership boundary for UI,
//! engine-control, playback, render, audio, background-job, and GPU-submission
//! execution. The [`jobs`] module provides deterministic weighted priority scheduling,
//! transparent derived-media selection, cancellation, deadlines, dependencies, progress, and
//! typed terminal results. Its bounded worker pool applies the same weighted policy across local
//! queues, executes cooperative jobs in the background-job domain, and makes owned and stolen
//! dispatches observable. The [`clock`] module provides anchor-based monotonic playback and exact
//! audio-master modes, measures exact A/V drift, and returns nonblocking playback-owned video wait,
//! correction, drop, and rebase decisions. The [`shared`] module keeps mutable state with one
//! transferable but non-shared domain owner and publishes immutable generation-tagged snapshots
//! without locks or payload copies. Backpressure and coordinated shutdown advance through their
//! focused checkpoints. See section 5.7 in `docs/architecture.md`.

pub mod clock;
pub mod jobs;
pub mod shared;
pub mod submit;
pub mod threads;
