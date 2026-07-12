//! `superi-audio`, audio processing graph, mixing, sample-accurate sync.
//!
//! § 5.9 in `docs/architecture.md`. Depends on: superi-core, superi-concurrency. Status: skeleton.

pub mod graph;
pub mod hosting;
pub mod metering;
pub mod mixing;
pub mod playback;
pub mod resample;
pub mod sync;
