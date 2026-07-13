//! macOS VideoToolbox and AudioConverter codec backend.

use std::sync::Arc;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_media_io::backend::{
    BackendCapabilities, BackendCapability, BackendDescriptor, BackendRegistration, BackendTier,
    ChromaSampling, CodecCapability, CodecOperation, HardwareAcceleration, MediaBackend,
};
use superi_media_io::decode::{Decoder, DecoderConfig};
use superi_media_io::demux::{
    BackendId, CodecId, MediaSource, SourceProbe, SourceProbeResult, SourceRequest,
};
use superi_media_io::encode::{Encoder, EncoderConfig};
use superi_media_io::operation::OperationContext;

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{parse_avcc, parse_hvcc, VideoCodecConfiguration, VideoToolboxFrameBuffer};

/// Stable H.264 codec identifier used by containers and selection.
pub const H264_CODEC_ID: &str = "h264";
/// Stable HEVC, also known as H.265, codec identifier used by containers and selection.
pub const HEVC_CODEC_ID: &str = "hevc";
/// Stable AAC codec identifier used by containers and selection.
pub const AAC_CODEC_ID: &str = "aac";

const COMPONENT: &str = "superi-codecs-platform.videotoolbox";

/// One launch ProRes profile supported through VideoToolbox decode and encode.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ProResProfile {
    /// Apple ProRes 422 Proxy.
    Proxy,
    /// Apple ProRes 422 LT.
    Lt,
    /// Apple ProRes 422.
    Standard,
    /// Apple ProRes 422 HQ.
    Hq,
    /// Apple ProRes 4444 with straight alpha.
    FourFourFourFour,
}

impl ProResProfile {
    /// Every profile implemented by this backend in stable capability order.
    pub const ALL: &'static [Self] = &[
        Self::Proxy,
        Self::Lt,
        Self::Standard,
        Self::Hq,
        Self::FourFourFourFour,
    ];

    /// Returns the permanent codec code for this profile.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Proxy => "prores-422-proxy",
            Self::Lt => "prores-422-lt",
            Self::Standard => "prores-422",
            Self::Hq => "prores-422-hq",
            Self::FourFourFourFour => "prores-4444",
        }
    }

    /// Returns the codec-neutral identifier for this profile.
    #[must_use]
    pub fn codec_id(self) -> CodecId {
        CodecId::new(self.code()).expect("static ProRes codec identifier is valid")
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn from_id(codec: &CodecId) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|profile| profile.code() == codec.as_str())
    }
}

/// Opt-in Apple media backend for encumbered launch codecs.
pub struct VideoToolboxBackend {
    descriptor: BackendDescriptor,
}

impl VideoToolboxBackend {
    /// Creates the backend with a stable platform identity.
    pub fn new() -> Result<Self> {
        Ok(Self {
            descriptor: BackendDescriptor::new(
                BackendId::new("apple-videotoolbox")?,
                "Apple VideoToolbox",
            )?,
        })
    }

    /// Builds the deterministic native decode and encode registration.
    pub fn registration() -> Result<BackendRegistration> {
        let mut capabilities = Vec::new();
        let mut codec_capabilities = Vec::new();
        for codec in [H264_CODEC_ID, HEVC_CODEC_ID, AAC_CODEC_ID]
            .into_iter()
            .map(|code| CodecId::new(code).expect("static codec identifier is valid"))
            .chain(ProResProfile::ALL.iter().map(|profile| profile.codec_id()))
        {
            capabilities.push(BackendCapability::Decode(codec.clone()));
            capabilities.push(BackendCapability::Encode(codec.clone()));
            for operation in [CodecOperation::Decode, CodecOperation::Encode] {
                codec_capabilities.push(videotoolbox_capability(operation, codec.clone())?);
            }
        }
        BackendRegistration::new(
            Arc::new(Self::new()?),
            BackendCapabilities::new(capabilities)
                .with_hardware_acceleration(HardwareAcceleration::PlatformManaged)
                .with_codec_capabilities(codec_capabilities)?,
            200,
            BackendTier::Primary,
        )
    }
}

fn videotoolbox_capability(operation: CodecOperation, codec: CodecId) -> Result<CodecCapability> {
    let value = CodecCapability::new(operation, codec.clone());
    match codec.as_str() {
        H264_CODEC_ID | HEVC_CODEC_ID => Ok(value
            .with_profiles_runtime()
            .with_levels_runtime()
            .with_bit_depths_runtime()
            .with_chroma_sampling_runtime()),
        AAC_CODEC_ID => Ok(value
            .with_profiles_runtime()
            .with_levels_not_applicable()
            .with_bit_depths_runtime()
            .with_chroma_sampling_not_applicable()),
        code => {
            let profile = ProResProfile::ALL
                .iter()
                .copied()
                .find(|profile| profile.code() == code)
                .expect("registration contains only known ProRes profiles");
            let bit_depth = if profile == ProResProfile::FourFourFourFour {
                12
            } else {
                10
            };
            value
                .with_profiles([profile.code()])
                .map(CodecCapability::with_levels_not_applicable)?
                .with_bit_depths([bit_depth])
                .and_then(|value| {
                    value.with_chroma_sampling([if profile == ProResProfile::FourFourFourFour {
                        ChromaSampling::Cs444
                    } else {
                        ChromaSampling::Cs422
                    }])
                })
        }
    }
}

impl MediaBackend for VideoToolboxBackend {
    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn probe_source(
        &self,
        _probe: &SourceProbe<'_>,
        operation: &OperationContext,
    ) -> Result<SourceProbeResult> {
        operation.check("probe_videotoolbox_source")?;
        Ok(SourceProbeResult::NoMatch)
    }

    fn open_source(
        &self,
        _request: &SourceRequest,
        operation: &OperationContext,
    ) -> Result<Box<dyn MediaSource>> {
        operation.check("open_videotoolbox_source")?;
        Err(unsupported(
            "open_videotoolbox_source",
            "the VideoToolbox codec backend does not open media containers",
        ))
    }

    fn create_decoder(
        &self,
        config: &DecoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Decoder>> {
        operation.check("create_videotoolbox_decoder")?;
        #[cfg(target_os = "macos")]
        {
            macos::create_decoder(config, operation)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = config;
            Err(unsupported(
                "create_videotoolbox_decoder",
                "the VideoToolbox backend is available only on macOS",
            ))
        }
    }

    fn create_encoder(
        &self,
        config: &EncoderConfig,
        operation: &OperationContext,
    ) -> Result<Box<dyn Encoder>> {
        operation.check("create_videotoolbox_encoder")?;
        #[cfg(target_os = "macos")]
        {
            macos::create_encoder(config, operation)
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = config;
            Err(unsupported(
                "create_videotoolbox_encoder",
                "the VideoToolbox backend is available only on macOS",
            ))
        }
    }
}

fn unsupported(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::Degraded,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
