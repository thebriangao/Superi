//! `superi-concurrency`, execution domains, deterministic job scheduling, clocks, and GPU
//! coordination.
//!
//! The [`threads`] module enforces the Phase 0 ownership boundary for UI,
//! engine-control, playback, render, audio, background-job, and GPU-submission
//! execution. The [`jobs`] module provides deterministic weighted priority scheduling,
//! transparent derived-media selection, cancellation, deadlines, dependencies, progress, and
//! typed terminal results. The [`clock`] module provides anchor-based monotonic playback and exact
//! audio-master clock modes. Work stealing, A/V drift correction, backpressure, shared-state
//! conventions, and coordinated shutdown advance through their focused checkpoints. See section
//! 5.7 in `docs/architecture.md`.

pub mod clock;
pub mod jobs;
pub mod shared;
pub mod submit;
pub mod threads;
