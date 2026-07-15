//! `superi-audio`, independent audio processing, device playback, mixing, and sample-accurate sync.
//!
//! The foundational editable and prepared processing graph is implemented in [`graph`]. Exact
//! timeline callback scheduling and audio-master publication are implemented in [`sync`]. Device
//! discovery and bounded realtime playback are implemented in [`playback`]. Mixing, resampling,
//! metering, and plugin hosting remain separate staged owners.

pub mod graph;
pub mod hosting;
pub mod metering;
pub mod mixing;
pub mod playback;
pub mod resample;
pub mod sync;
