//! Prepared macOS Audio Unit effect hosting.
//!
//! Configuration and component identity are portable safe values. Native discovery, preparation,
//! callback ownership, and rendering are private to the macOS boundary. Preparation is confined to
//! the blocking background domain, while a prepared effect implements the ordinary allocation-free
//! [`crate::graph::AudioProcessor`] contract.

use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::ChannelLayout;

use crate::graph::{AudioProcessBlock, AudioProcessor};

#[cfg(target_os = "macos")]
mod audio_unit_macos;

const COMPONENT: &str = "superi-audio.hosting.audio-unit";
const EFFECT_COMPONENT_TYPE: [u8; 4] = *b"aufx";
const MAX_CHANNELS: usize = 64;

/// Exact Audio Unit effect component identity.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AudioUnitComponentId {
    component_type: [u8; 4],
    subtype: [u8; 4],
    manufacturer: [u8; 4],
}

impl AudioUnitComponentId {
    /// Creates an effect identity from its subtype and manufacturer FourCC values.
    pub fn effect(subtype: [u8; 4], manufacturer: [u8; 4]) -> Result<Self> {
        if u32::from_be_bytes(subtype) == 0 || u32::from_be_bytes(manufacturer) == 0 {
            return Err(error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "create_component_identity",
                "Audio Unit subtype and manufacturer must be nonzero FourCC values",
            ));
        }
        Ok(Self {
            component_type: EFFECT_COMPONENT_TYPE,
            subtype,
            manufacturer,
        })
    }

    /// Returns the component type FourCC bytes.
    #[must_use]
    pub const fn component_type(self) -> [u8; 4] {
        self.component_type
    }

    /// Returns the component subtype FourCC bytes.
    #[must_use]
    pub const fn subtype(self) -> [u8; 4] {
        self.subtype
    }

    /// Returns the component manufacturer FourCC bytes.
    #[must_use]
    pub const fn manufacturer(self) -> [u8; 4] {
        self.manufacturer
    }

    /// Returns the native big-endian component type value.
    #[must_use]
    pub const fn raw_component_type(self) -> u32 {
        u32::from_be_bytes(self.component_type)
    }

    /// Returns the native big-endian component subtype value.
    #[must_use]
    pub const fn raw_subtype(self) -> u32 {
        u32::from_be_bytes(self.subtype)
    }

    /// Returns the native big-endian component manufacturer value.
    #[must_use]
    pub const fn raw_manufacturer(self) -> u32 {
        u32::from_be_bytes(self.manufacturer)
    }
}

/// Process-location policy for Audio Unit instantiation.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum AudioUnitExecutionPolicy {
    /// Require the operating system to load the component outside the host process.
    #[default]
    RequireOutOfProcess,
    /// Permit a component audited by the caller to run in the host process.
    AllowAuditedInProcess,
}

/// Immutable preparation contract for one Audio Unit effect instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioUnitHostConfig {
    component: AudioUnitComponentId,
    sample_rate: u32,
    maximum_frames: usize,
    input_layout: ChannelLayout,
    output_layout: ChannelLayout,
    execution_policy: AudioUnitExecutionPolicy,
}

impl AudioUnitHostConfig {
    /// Creates and validates one exact effect-hosting contract.
    pub fn new(
        component: AudioUnitComponentId,
        sample_rate: u32,
        maximum_frames: usize,
        input_layout: ChannelLayout,
        output_layout: ChannelLayout,
        execution_policy: AudioUnitExecutionPolicy,
    ) -> Result<Self> {
        if sample_rate == 0 {
            return Err(invalid_config("sample rate must be greater than zero"));
        }
        if maximum_frames == 0 || u32::try_from(maximum_frames).is_err() {
            return Err(invalid_config(
                "maximum frame count must fit a positive native frame count",
            ));
        }
        if input_layout != output_layout {
            return Err(invalid_config(
                "Audio Unit effect input and output layouts must match exactly",
            ));
        }
        if input_layout.len() > MAX_CHANNELS {
            return Err(error(
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
                "create_host_config",
                "Audio Unit layout exceeds the prepared channel capacity",
            ));
        }
        maximum_frames
            .checked_mul(input_layout.len())
            .and_then(|samples| samples.checked_mul(std::mem::size_of::<f32>()))
            .and_then(|bytes| u32::try_from(bytes).ok())
            .ok_or_else(|| {
                error(
                    ErrorCategory::ResourceExhausted,
                    Recoverability::UserCorrectable,
                    "create_host_config",
                    "Audio Unit scratch buffer size overflowed",
                )
            })?;
        Ok(Self {
            component,
            sample_rate,
            maximum_frames,
            input_layout,
            output_layout,
            execution_policy,
        })
    }

    /// Returns the exact component identity.
    #[must_use]
    pub const fn component(&self) -> AudioUnitComponentId {
        self.component
    }

    /// Returns the prepared sample rate.
    #[must_use]
    pub const fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Returns the largest accepted process slice.
    #[must_use]
    pub const fn maximum_frames(&self) -> usize {
        self.maximum_frames
    }

    /// Returns the exact ordered input channel layout.
    #[must_use]
    pub const fn input_layout(&self) -> &ChannelLayout {
        &self.input_layout
    }

    /// Returns the exact ordered output channel layout.
    #[must_use]
    pub const fn output_layout(&self) -> &ChannelLayout {
        &self.output_layout
    }

    /// Returns the requested process-location policy.
    #[must_use]
    pub const fn execution_policy(&self) -> AudioUnitExecutionPolicy {
        self.execution_policy
    }
}

/// One initialized Audio Unit effect with stable storage for real-time processing.
#[derive(Debug)]
pub struct PreparedAudioUnit {
    config: AudioUnitHostConfig,
    #[cfg(target_os = "macos")]
    native: audio_unit_macos::PreparedAudioUnit,
}

impl PreparedAudioUnit {
    /// Discovers and initializes the exact component on a blocking background worker.
    pub fn prepare(config: AudioUnitHostConfig) -> Result<Self> {
        ExecutionDomain::BackgroundJob
            .require_current()
            .map_err(|mut error| {
                error.push_context(ErrorContext::new(COMPONENT, "prepare_audio_unit"));
                error
            })?;

        #[cfg(target_os = "macos")]
        {
            let native = audio_unit_macos::PreparedAudioUnit::prepare(&config)?;
            Ok(Self { config, native })
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = config;
            Err(error(
                ErrorCategory::Unsupported,
                Recoverability::UserCorrectable,
                "prepare_audio_unit",
                "Audio Unit hosting is available only on macOS",
            ))
        }
    }

    /// Returns the exact component identity of the native instance.
    #[must_use]
    pub const fn component(&self) -> AudioUnitComponentId {
        self.config.component
    }

    /// Returns the native component version read during preparation.
    #[must_use]
    pub fn component_version(&self) -> u32 {
        #[cfg(target_os = "macos")]
        {
            self.native.component_version()
        }
        #[cfg(not(target_os = "macos"))]
        {
            0
        }
    }

    /// Returns whether the operating system loaded this instance outside the host process.
    #[must_use]
    pub fn loaded_out_of_process(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.native.loaded_out_of_process()
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }

    /// Returns whether a native-entry failure made the instance unsafe to reuse.
    #[must_use]
    pub fn is_poisoned(&self) -> bool {
        #[cfg(target_os = "macos")]
        {
            self.native.is_poisoned()
        }
        #[cfg(not(target_os = "macos"))]
        {
            false
        }
    }
}

impl AudioProcessor for PreparedAudioUnit {
    fn process(&mut self, block: AudioProcessBlock<'_>) -> Result<()> {
        #[cfg(target_os = "macos")]
        {
            self.native.process(&self.config, block)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = block;
            Err(error(
                ErrorCategory::Unsupported,
                Recoverability::Terminal,
                "process_audio_unit",
                "Audio Unit hosting is available only on macOS",
            ))
        }
    }
}

fn invalid_config(message: &'static str) -> Error {
    error(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        "create_host_config",
        message,
    )
}

fn error(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
