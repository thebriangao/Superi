//! Focused imports for contracts shared across Superi subsystems.
//!
//! The prelude is an explicit allowlist of canonical types. Every item remains
//! owned by its named module, so importing it here does not introduce a wrapper,
//! a second serialization contract, or alternate runtime state. Existing module
//! paths remain stable and are the right choice when a consumer needs specialized
//! parser details or raw diagnostic failure data.
//!
//! Wildcard imports from this module are intentionally predictable. New public
//! items in an owning module do not enter the prelude until they are deliberately
//! reviewed as broadly shared contracts.
//!
//! # Example
//!
//! ```
//! use std::str::FromStr;
//! use superi_core::prelude::*;
//!
//! let project = ProjectId::from_raw(42);
//! let rate = FrameRate::FPS_24000_1001;
//! let setting = SettingKey::from_str("superi.playback.loop").unwrap();
//! let settings = SettingsSnapshot::new(
//!     SemanticVersion::from_str("1.0.0").unwrap(),
//!     [(setting, SettingValue::Boolean(true))],
//! )
//! .unwrap();
//!
//! assert_eq!(project.to_string(), "project:0000000000000000000000000000002a");
//! assert_eq!(RationalTime::from_frames(24, rate).value(), 24);
//! assert_eq!(settings.len(), 1);
//! assert_eq!(STABLE_PRIMITIVE_SCHEMA_REVISION, 1);
//! ```
//!
//! Construction-specific errors and internal failure snapshots stay on targeted
//! module paths and do not enter an ordinary wildcard import.
//!
//! ```compile_fail
//! use superi_core::prelude::ParseIdentifierError;
//! ```
//!
//! ```compile_fail
//! use superi_core::prelude::FailureDiagnostic;
//! ```
//!
//! ```compile_fail
//! use superi_core::prelude::VersionSection;
//! ```

pub use crate::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
pub use crate::diagnostics::{
    CounterSnapshot, CounterUnit, DiagnosticEvent, DiagnosticSeverity, FieldVisibility, FiniteF64,
    PerformanceCounter, TraceField, TraceValue, UserSafeError,
};
pub use crate::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result, ResultExt};
pub use crate::geometry::{AspectRatio, Matrix3, PixelBounds, Point2, Rect, Vector2};
pub use crate::ids::{
    CacheId, CaptionId, ClipId, DeviceId, EdgeId, GapId, GeneratorId, GraphId, IdentifierKind,
    JobId, MarkerId, MediaId, NodeId, ParameterId, PortId, ProjectId, ResourceId, TimelineId,
    TrackId, TransitionId, TypedId,
};
pub use crate::pixel::{
    AlphaMode, ChannelLayout, ChannelPosition, ChromaSubsampling, PixelFormat, PixelModel,
    PixelNumeric, PixelPacking, SampleFormat, SampleNumeric,
};
pub use crate::serialization::STABLE_PRIMITIVE_SCHEMA_REVISION;
pub use crate::settings::{
    CapabilityId, CapabilitySet, ComponentId, FeatureAvailability, FeatureDescriptor,
    FeatureDiscovery, FeatureId, SemanticVersion, SettingKey, SettingValue, SettingValueKind,
    SettingsSnapshot, VersionIdentifier,
};
pub use crate::time::{
    Duration, FrameRate, RationalTime, SampleTime, TimeRange, TimeRounding, Timebase,
};
pub use crate::timecode::{Timecode, TimecodeFormat, TimecodeMode};
