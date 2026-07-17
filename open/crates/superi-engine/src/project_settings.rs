//! Typed runtime resolution of durable project settings.
//!
//! Resolution is read-only. It maps one validated project snapshot into the
//! exact types already owned by timeline, color identity, audio, cache, proxy,
//! and render subsystems. Authored timeline and graph state is never rewritten.

use superi_cache::eviction::CacheBudgetLimit;
use superi_cache::key::RenderSettingsFingerprint;
use superi_cache::proxy::DerivedMediaQuality;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::ProjectId;
use superi_core::pixel::{AlphaMode, ChannelLayout, PixelFormat};
use superi_core::settings::SettingValue;
use superi_core::time::{FrameRate, Timebase};
use superi_core::timecode::TimecodeFormat;
use superi_project::document::ProjectSnapshot;
use superi_project::settings::{
    ProjectSettings, AUDIO_OUTPUT_LAYOUT_KEY, AUDIO_SAMPLE_RATE_KEY, CACHE_MAX_BYTES_KEY,
    CACHE_MAX_FRAMES_KEY, CACHE_MODE_KEY, COLOR_CONFIG_FINGERPRINT_KEY, COLOR_CONFIG_ID_KEY,
    COLOR_MODE_KEY, COLOR_WORKING_SPACE_KEY, PROXY_MODE_KEY, PROXY_QUALITY_KEY,
    RENDER_ALPHA_MODE_KEY, RENDER_COLOR_TARGET_KEY, RENDER_EXTENT_MODE_KEY,
    RENDER_FRAME_RATE_MODE_KEY, RENDER_HEIGHT_KEY, RENDER_PIXEL_FORMAT_KEY,
    RENDER_RATE_DENOMINATOR_KEY, RENDER_RATE_NUMERATOR_KEY, RENDER_WIDTH_KEY,
    TIMELINE_RATE_DENOMINATOR_KEY, TIMELINE_RATE_NUMERATOR_KEY, TIMELINE_TIMECODE_MODE_KEY,
};

pub use superi_project::settings::{ProjectSettingMutation, ProjectSettingsTransaction};

const COMPONENT: &str = "superi-engine.project-settings";

/// Complete engine-facing state for one project settings revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectSettingsState {
    project_id: ProjectId,
    project_revision: u64,
    settings: ProjectSettings,
    resolved: ResolvedProjectSettings,
}

impl ProjectSettingsState {
    /// Resolves one immutable authoritative project snapshot.
    pub fn from_snapshot(snapshot: &ProjectSnapshot) -> Result<Self> {
        Ok(Self {
            project_id: snapshot.project_id(),
            project_revision: snapshot.revision(),
            settings: snapshot.settings().clone(),
            resolved: ResolvedProjectSettings::resolve(snapshot.settings())?,
        })
    }

    #[must_use]
    pub const fn project_id(&self) -> ProjectId {
        self.project_id
    }

    #[must_use]
    pub const fn project_revision(&self) -> u64 {
        self.project_revision
    }

    #[must_use]
    pub const fn settings(&self) -> &ProjectSettings {
        &self.settings
    }

    #[must_use]
    pub const fn resolved(&self) -> &ResolvedProjectSettings {
        &self.resolved
    }
}

/// Complete typed runtime policy derived from one settings snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedProjectSettings {
    timeline: TimelineSettings,
    color: ColorSettings,
    audio: AudioOutputSettings,
    cache: CachePolicy,
    proxy: ProxySettings,
    render: RenderSettings,
}

impl ResolvedProjectSettings {
    fn resolve(settings: &ProjectSettings) -> Result<Self> {
        Ok(Self {
            timeline: resolve_timeline(settings)?,
            color: resolve_color(settings)?,
            audio: resolve_audio(settings)?,
            cache: resolve_cache(settings)?,
            proxy: resolve_proxy(settings)?,
            render: resolve_render(settings)?,
        })
    }

    #[must_use]
    pub const fn timeline(&self) -> &TimelineSettings {
        &self.timeline
    }

    #[must_use]
    pub const fn color(&self) -> &ColorSettings {
        &self.color
    }

    #[must_use]
    pub const fn audio(&self) -> &AudioOutputSettings {
        &self.audio
    }

    #[must_use]
    pub const fn cache(&self) -> &CachePolicy {
        &self.cache
    }

    #[must_use]
    pub const fn proxy(&self) -> &ProxySettings {
        &self.proxy
    }

    #[must_use]
    pub const fn render(&self) -> &RenderSettings {
        &self.render
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimelineSettings {
    frame_rate: FrameRate,
    timecode: TimecodeFormat,
}

impl TimelineSettings {
    #[must_use]
    pub const fn frame_rate(self) -> FrameRate {
        self.frame_rate
    }

    #[must_use]
    pub const fn timecode(self) -> TimecodeFormat {
        self.timecode
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ColorSettings {
    BuiltInAcesCg {
        working_space: String,
    },
    PinnedConfig {
        config_id: String,
        config_fingerprint: String,
        working_space: String,
    },
}

impl ColorSettings {
    #[must_use]
    pub fn working_space(&self) -> &str {
        match self {
            Self::BuiltInAcesCg { working_space } | Self::PinnedConfig { working_space, .. } => {
                working_space
            }
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioOutputSettings {
    sample_rate: Timebase,
    channel_layout: ChannelLayout,
}

impl AudioOutputSettings {
    #[must_use]
    pub const fn sample_rate(&self) -> Timebase {
        self.sample_rate
    }

    #[must_use]
    pub const fn channel_layout(&self) -> &ChannelLayout {
        &self.channel_layout
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CachePolicy {
    Automatic,
    Disabled,
    Bounded(CacheBudgetLimit),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ProxyMode {
    Disabled,
    OnDemand,
    Prefer,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProxySettings {
    mode: ProxyMode,
    quality: DerivedMediaQuality,
}

impl ProxySettings {
    #[must_use]
    pub const fn mode(self) -> ProxyMode {
        self.mode
    }

    #[must_use]
    pub const fn quality(self) -> DerivedMediaQuality {
        self.quality
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RenderColorTarget {
    ProjectWorking,
    Display,
    Delivery,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RenderSettings {
    frame_rate: Option<FrameRate>,
    extent: Option<(u32, u32)>,
    pixel_format: PixelFormat,
    alpha_mode: AlphaMode,
    color_target: RenderColorTarget,
    fingerprint: RenderSettingsFingerprint,
}

impl RenderSettings {
    #[must_use]
    pub const fn frame_rate(self) -> Option<FrameRate> {
        self.frame_rate
    }

    #[must_use]
    pub const fn extent(self) -> Option<(u32, u32)> {
        self.extent
    }

    #[must_use]
    pub const fn pixel_format(self) -> PixelFormat {
        self.pixel_format
    }

    #[must_use]
    pub const fn alpha_mode(self) -> AlphaMode {
        self.alpha_mode
    }

    #[must_use]
    pub const fn color_target(self) -> RenderColorTarget {
        self.color_target
    }

    #[must_use]
    pub const fn fingerprint(self) -> RenderSettingsFingerprint {
        self.fingerprint
    }
}

fn resolve_timeline(settings: &ProjectSettings) -> Result<TimelineSettings> {
    let frame_rate = FrameRate::new(
        u32_value(settings, TIMELINE_RATE_NUMERATOR_KEY)?,
        u32_value(settings, TIMELINE_RATE_DENOMINATOR_KEY)?,
    )?;
    let timecode = match text_value(settings, TIMELINE_TIMECODE_MODE_KEY)? {
        "non_drop_frame" => TimecodeFormat::non_drop(frame_rate),
        "drop_frame" => TimecodeFormat::drop_frame(frame_rate).map_err(|_| {
            internal(
                "resolve_timeline",
                "validated drop-frame project settings could not be resolved",
            )
        })?,
        _ => return Err(validated_value_changed(TIMELINE_TIMECODE_MODE_KEY)),
    };
    Ok(TimelineSettings {
        frame_rate,
        timecode,
    })
}

fn resolve_color(settings: &ProjectSettings) -> Result<ColorSettings> {
    let working_space = text_value(settings, COLOR_WORKING_SPACE_KEY)?.to_owned();
    match text_value(settings, COLOR_MODE_KEY)? {
        "built_in_acescg" => Ok(ColorSettings::BuiltInAcesCg { working_space }),
        "pinned_config" => Ok(ColorSettings::PinnedConfig {
            config_id: text_value(settings, COLOR_CONFIG_ID_KEY)?.to_owned(),
            config_fingerprint: text_value(settings, COLOR_CONFIG_FINGERPRINT_KEY)?.to_owned(),
            working_space,
        }),
        _ => Err(validated_value_changed(COLOR_MODE_KEY)),
    }
}

fn resolve_audio(settings: &ProjectSettings) -> Result<AudioOutputSettings> {
    let sample_rate = Timebase::integer(u32_value(settings, AUDIO_SAMPLE_RATE_KEY)?)?;
    let channel_layout = match text_value(settings, AUDIO_OUTPUT_LAYOUT_KEY)? {
        "mono" => ChannelLayout::mono(),
        "stereo" => ChannelLayout::stereo(),
        "quad" => ChannelLayout::quad(),
        "surround_5_1" => ChannelLayout::surround_5_1(),
        "surround_7_1" => ChannelLayout::surround_7_1(),
        _ => return Err(validated_value_changed(AUDIO_OUTPUT_LAYOUT_KEY)),
    };
    Ok(AudioOutputSettings {
        sample_rate,
        channel_layout,
    })
}

fn resolve_cache(settings: &ProjectSettings) -> Result<CachePolicy> {
    match text_value(settings, CACHE_MODE_KEY)? {
        "automatic" => Ok(CachePolicy::Automatic),
        "disabled" => Ok(CachePolicy::Disabled),
        "bounded" => Ok(CachePolicy::Bounded(CacheBudgetLimit::new(
            u64_value(settings, CACHE_MAX_BYTES_KEY)?,
            u64_value(settings, CACHE_MAX_FRAMES_KEY)?,
        )?)),
        _ => Err(validated_value_changed(CACHE_MODE_KEY)),
    }
}

fn resolve_proxy(settings: &ProjectSettings) -> Result<ProxySettings> {
    let mode = match text_value(settings, PROXY_MODE_KEY)? {
        "disabled" => ProxyMode::Disabled,
        "on_demand" => ProxyMode::OnDemand,
        "prefer" => ProxyMode::Prefer,
        _ => return Err(validated_value_changed(PROXY_MODE_KEY)),
    };
    let quality = match text_value(settings, PROXY_QUALITY_KEY)? {
        "eighth" => DerivedMediaQuality::Eighth,
        "quarter" => DerivedMediaQuality::Quarter,
        "half" => DerivedMediaQuality::Half,
        "full" => DerivedMediaQuality::Full,
        _ => return Err(validated_value_changed(PROXY_QUALITY_KEY)),
    };
    Ok(ProxySettings { mode, quality })
}

fn resolve_render(settings: &ProjectSettings) -> Result<RenderSettings> {
    let frame_rate = match text_value(settings, RENDER_FRAME_RATE_MODE_KEY)? {
        "timeline" => None,
        "explicit" => Some(FrameRate::new(
            u32_value(settings, RENDER_RATE_NUMERATOR_KEY)?,
            u32_value(settings, RENDER_RATE_DENOMINATOR_KEY)?,
        )?),
        _ => return Err(validated_value_changed(RENDER_FRAME_RATE_MODE_KEY)),
    };
    let extent = match text_value(settings, RENDER_EXTENT_MODE_KEY)? {
        "source" => None,
        "explicit" => Some((
            u32_value(settings, RENDER_WIDTH_KEY)?,
            u32_value(settings, RENDER_HEIGHT_KEY)?,
        )),
        _ => return Err(validated_value_changed(RENDER_EXTENT_MODE_KEY)),
    };
    let pixel_format = match text_value(settings, RENDER_PIXEL_FORMAT_KEY)? {
        "rgba16_float" => PixelFormat::Rgba16Float,
        "rgba32_float" => PixelFormat::Rgba32Float,
        _ => return Err(validated_value_changed(RENDER_PIXEL_FORMAT_KEY)),
    };
    let alpha_mode = match text_value(settings, RENDER_ALPHA_MODE_KEY)? {
        "opaque" => AlphaMode::Opaque,
        "straight" => AlphaMode::Straight,
        "premultiplied" => AlphaMode::Premultiplied,
        _ => return Err(validated_value_changed(RENDER_ALPHA_MODE_KEY)),
    };
    let color_target = match text_value(settings, RENDER_COLOR_TARGET_KEY)? {
        "project_working" => RenderColorTarget::ProjectWorking,
        "display" => RenderColorTarget::Display,
        "delivery" => RenderColorTarget::Delivery,
        _ => return Err(validated_value_changed(RENDER_COLOR_TARGET_KEY)),
    };
    Ok(RenderSettings {
        frame_rate,
        extent,
        pixel_format,
        alpha_mode,
        color_target,
        fingerprint: RenderSettingsFingerprint::from_canonical_bytes(canonical_settings_bytes(
            settings,
        )),
    })
}

fn canonical_settings_bytes(settings: &ProjectSettings) -> Vec<u8> {
    let mut bytes = b"superi.engine.project-settings.render.v1\0".to_vec();
    for (key, value) in settings.iter() {
        append_field(&mut bytes, key.as_str().as_bytes());
        match value {
            SettingValue::Boolean(value) => {
                bytes.push(1);
                append_field(&mut bytes, &[*value as u8]);
            }
            SettingValue::Integer(value) => {
                bytes.push(2);
                append_field(&mut bytes, &value.to_be_bytes());
            }
            SettingValue::Text(value) => {
                bytes.push(3);
                append_field(&mut bytes, value.as_bytes());
            }
            _ => unreachable!("project settings validation rejects unknown value kinds"),
        }
    }
    bytes
}

fn append_field(bytes: &mut Vec<u8>, field: &[u8]) {
    bytes.extend_from_slice(&(field.len() as u64).to_be_bytes());
    bytes.extend_from_slice(field);
}

fn text_value<'a>(settings: &'a ProjectSettings, key: &str) -> Result<&'a str> {
    settings
        .text(key)
        .ok_or_else(|| validated_value_changed(key))
}

fn u32_value(settings: &ProjectSettings, key: &str) -> Result<u32> {
    settings
        .integer(key)
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| validated_value_changed(key))
}

fn u64_value(settings: &ProjectSettings, key: &str) -> Result<u64> {
    settings
        .integer(key)
        .and_then(|value| u64::try_from(value).ok())
        .ok_or_else(|| validated_value_changed(key))
}

fn validated_value_changed(key: &str) -> Error {
    internal(
        "resolve_project_settings",
        "validated project setting could not be resolved",
    )
    .with_context(ErrorContext::new(COMPONENT, "resolve_project_settings").with_field("key", key))
}

fn internal(operation: &'static str, message: &'static str) -> Error {
    Error::new(ErrorCategory::Internal, Recoverability::Terminal, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
