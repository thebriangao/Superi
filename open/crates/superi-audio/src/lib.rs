//! `superi-audio`, independent audio processing, mixing, and sample-accurate sync.
//!
//! The foundational editable and prepared processing graph is implemented in [`graph`]. Playback,
//! mixing, synchronization, resampling, metering, and plugin hosting remain separate staged owners.

pub mod graph;
pub mod hosting;
pub mod metering;
pub mod mixing;
pub mod playback;
pub mod resample;
pub mod sync;
