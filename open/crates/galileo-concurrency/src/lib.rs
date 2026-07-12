//! `galileo-concurrency`, job system, threading, clock, A/V-sync scheduling.
//!
//! § 5.7 in `docs/architecture.md`. Depends on: galileo-core, galileo-gpu. Status: skeleton.

pub mod clock;
pub mod jobs;
pub mod shared;
pub mod submit;
pub mod threads;
