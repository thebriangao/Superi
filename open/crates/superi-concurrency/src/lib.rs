//! `superi-concurrency`, job system, threading, clock, A/V-sync scheduling.
//!
//! The job lifecycle contracts provide cancellation, deadlines, dependencies, progress, and typed
//! terminal results. Execution domains, queue scheduling, work stealing, clocks, backpressure,
//! shared-state conventions, and coordinated shutdown advance through their focused checkpoints.
//! See section 5.7 in `docs/architecture.md`.

pub mod clock;
pub mod jobs;
pub mod shared;
pub mod submit;
pub mod threads;
