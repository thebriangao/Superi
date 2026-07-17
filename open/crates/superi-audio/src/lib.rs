//! `superi-audio`, independent audio processing, device I/O, mixing, and sample-accurate sync.
//!
//! The foundational editable and prepared processing graph is implemented in [`graph`]. Exact
//! timeline callback scheduling and audio-master publication are implemented in [`sync`]. Device
//! discovery and bounded realtime playback are implemented in [`playback`]. Sample-accurate clip
//! controls and transactional mix intent are implemented in [`mixing`]. Revisioned clip-gain
//! keyframes and professional automation modes are implemented in [`automation`]. Typed buses and
//! unity routing are implemented in [`routing`]. Explicit prepared layout conversion is implemented in
//! [`channels`]. Prepared band-limited conversion between independent source and device sample
//! clocks is implemented in [`resample`]. Prepared equalization, dynamics, limiting, delay, and
//! saturation are implemented in [`effects`]. Transparent graph-native level,
//! spectrum, phase, true-peak, and loudness analysis is implemented in [`metering`]. Bounded record
//! arming, input monitoring, and operating-system capture are implemented in [`capture`]. Prepared
//! macOS Audio Unit effect hosting is implemented in [`hosting`], while VST3, instruments, MIDI,
//! and plug-in UI remain separate staged owners.

pub mod automation;
pub mod capture;
pub mod channels;
pub mod effects;
pub mod graph;
pub mod hosting;
pub mod metering;
pub mod mixing;
pub mod playback;
pub mod resample;
pub mod routing;
pub mod serialize;
pub mod sync;
