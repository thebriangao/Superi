//! Deterministic identities for reusable frame and intermediate results.
//!
//! Cache keys compose identities owned by their authoritative subsystems. The graph evaluator owns
//! graph lineage, image metadata owns color meaning, core owns media IDs and physical time, and the
//! caller owns canonical parameter and render-setting encodings. This module supplies the stable
//! byte contract that joins them without owning cache storage or evaluation policy.

use std::fmt;

use sha2::{Digest, Sha256};
use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::MediaId;
use superi_core::time::RationalTime;
use superi_graph::diagnostics::EvaluationCacheKey;
use superi_image::metadata::ColorPipelineMetadata;

const MEDIA_CONTENT_DOMAIN: &[u8] = b"superi.cache.media-content.v1\0";
const PARAMETER_STATE_DOMAIN: &[u8] = b"superi.cache.parameter-state.v1\0";
const RENDER_SETTINGS_DOMAIN: &[u8] = b"superi.cache.render-settings.v1\0";
const COLOR_PIPELINE_DOMAIN: &[u8] = b"superi.cache.color-pipeline.v1\0";
const FRAME_CACHE_KEY_DOMAIN: &[u8] = b"superi.cache.frame-key.v1\0";

/// Stable project media identity plus an exact content fingerprint digest.
///
/// This is a cache ingredient, not a second media source model. The engine creates it from the
/// authoritative media source identity at the orchestration boundary because the reviewed crate
/// graph intentionally prevents `superi-cache` from depending on `superi-media-io`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct MediaCacheIdentity {
    media_id: MediaId,
    content_fingerprint: [u8; 32],
}

impl MediaCacheIdentity {
    /// Hashes a nonempty backend content fingerprint beside its persistent project media ID.
    pub fn new(media_id: MediaId, content_fingerprint: impl AsRef<str>) -> Result<Self> {
        let content_fingerprint = content_fingerprint.as_ref();
        if content_fingerprint.trim().is_empty() {
            return Err(Error::new(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "cache media content fingerprint must not be empty",
            )
            .with_context(ErrorContext::new(
                "superi-cache.key",
                "create_media_identity",
            )));
        }
        Ok(Self {
            media_id,
            content_fingerprint: digest_bytes(MEDIA_CONTENT_DOMAIN, content_fingerprint.as_bytes()),
        })
    }

    /// Returns the persistent project media identifier.
    #[must_use]
    pub const fn media_id(self) -> MediaId {
        self.media_id
    }

    /// Returns the domain-separated digest of the exact backend fingerprint text.
    #[must_use]
    pub const fn content_fingerprint(&self) -> &[u8; 32] {
        &self.content_fingerprint
    }
}

macro_rules! canonical_fingerprint {
    ($name:ident, $domain:ident, $summary:literal, $contract:literal) => {
        #[doc = $summary]
        #[doc = ""]
        #[doc = $contract]
        #[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
        pub struct $name([u8; 32]);

        impl $name {
            /// Hashes caller-owned canonical bytes with this value's versioned domain separator.
            #[must_use]
            pub fn from_canonical_bytes(bytes: impl AsRef<[u8]>) -> Self {
                Self(digest_bytes($domain, bytes.as_ref()))
            }

            /// Wraps a previously computed SHA-256 digest of the same canonical-state contract.
            #[must_use]
            pub const fn from_sha256(digest: [u8; 32]) -> Self {
                Self(digest)
            }

            /// Returns the exact digest bytes.
            #[must_use]
            pub const fn as_bytes(&self) -> &[u8; 32] {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                write_digest(formatter, &self.0)
            }
        }
    };
}

canonical_fingerprint!(
    ParameterStateFingerprint,
    PARAMETER_STATE_DOMAIN,
    "A digest of every canonical evaluated parameter byte that can affect one result.",
    "The caller must include resolved values, expressions, animation samples, seeds, and any other parameter-owned state that can change output."
);

canonical_fingerprint!(
    RenderSettingsFingerprint,
    RENDER_SETTINGS_DOMAIN,
    "A digest of every canonical render-setting byte that can affect one result.",
    "The caller must include artifact purpose, dimensions, pixel and alpha formats, precision, quality, proxy policy, sampling, backend-sensitive behavior, and every other output-affecting render choice."
);

/// A deterministic digest of complete color source identity, processing history, and output intent.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct ColorPipelineFingerprint([u8; 32]);

impl ColorPipelineFingerprint {
    /// Derives a digest from every semantic field of the image-owned color pipeline.
    #[must_use]
    pub fn derive(pipeline: &ColorPipelineMetadata) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(COLOR_PIPELINE_DOMAIN);

        let source = pipeline.source_tags();
        update_color_space(&mut hasher, source.interpretation());
        update_optional_text(&mut hasher, source.named_space());
        update_optional_bytes(&mut hasher, source.icc_profile());

        update_color_space(&mut hasher, pipeline.current_space());
        update_count(&mut hasher, pipeline.stages().len());
        for stage in pipeline.stages() {
            update_text(&mut hasher, stage.kind().code());
            update_text(&mut hasher, stage.transform_id());
            update_color_space(&mut hasher, stage.source());
            update_color_space(&mut hasher, stage.destination());
        }
        update_optional_color_space(&mut hasher, pipeline.working_space());
        update_optional_color_space(&mut hasher, pipeline.display_space());
        update_optional_color_space(&mut hasher, pipeline.delivery_space());

        Self(hasher.finalize().into())
    }

    /// Returns the exact digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for ColorPipelineFingerprint {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_digest(formatter, &self.0)
    }
}

/// Complete borrowed inputs for one reusable frame or intermediate-output identity.
#[derive(Clone, Copy, Debug)]
pub struct FrameCacheKeyInputs<'a> {
    media: &'a [MediaCacheIdentity],
    graph: EvaluationCacheKey,
    parameters: ParameterStateFingerprint,
    color: &'a ColorPipelineMetadata,
    time: RationalTime,
    render_settings: RenderSettingsFingerprint,
}

impl<'a> FrameCacheKeyInputs<'a> {
    /// Creates a complete key input set without interpreting caller-owned canonical state.
    #[must_use]
    pub const fn new(
        media: &'a [MediaCacheIdentity],
        graph: EvaluationCacheKey,
        parameters: ParameterStateFingerprint,
        color: &'a ColorPipelineMetadata,
        time: RationalTime,
        render_settings: RenderSettingsFingerprint,
    ) -> Self {
        Self {
            media,
            graph,
            parameters,
            color,
            time,
            render_settings,
        }
    }
}

/// One versioned SHA-256 identity for a reusable frame or intermediate graph output.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct FrameCacheKey([u8; 32]);

impl FrameCacheKey {
    /// Derives a key from all result-affecting identity categories.
    ///
    /// Media identities are a set: traversal order and exact duplicates do not change the key.
    /// Graph topology and input order remain represented by the authoritative graph evaluation
    /// key. An empty media set is valid for generated results whose lineage is entirely graph-owned.
    #[must_use]
    pub fn derive(inputs: FrameCacheKeyInputs<'_>) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(FRAME_CACHE_KEY_DOMAIN);

        let mut media = inputs.media.to_vec();
        media.sort_unstable();
        media.dedup();
        update_count(&mut hasher, media.len());
        for identity in media {
            hasher.update(identity.media_id().to_bytes());
            hasher.update(identity.content_fingerprint());
        }

        hasher.update(inputs.graph.as_bytes());
        hasher.update(inputs.parameters.as_bytes());
        hasher.update(ColorPipelineFingerprint::derive(inputs.color).as_bytes());
        update_physical_time(&mut hasher, inputs.time);
        hasher.update(inputs.render_settings.as_bytes());

        Self(hasher.finalize().into())
    }

    /// Returns the exact digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for FrameCacheKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write_digest(formatter, &self.0)
    }
}

fn digest_bytes(domain: &[u8], bytes: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    update_bytes(&mut hasher, bytes);
    hasher.finalize().into()
}

fn update_color_space(hasher: &mut Sha256, color_space: ColorSpace) {
    update_text(hasher, color_space.primaries().code());
    update_text(hasher, color_space.transfer().code());
    update_text(hasher, color_space.matrix().code());
    update_text(hasher, color_space.range().code());
}

fn update_optional_color_space(hasher: &mut Sha256, value: Option<ColorSpace>) {
    match value {
        Some(color_space) => {
            hasher.update([1]);
            update_color_space(hasher, color_space);
        }
        None => hasher.update([0]),
    }
}

fn update_optional_text(hasher: &mut Sha256, value: Option<&str>) {
    update_optional_bytes(hasher, value.map(str::as_bytes));
}

fn update_optional_bytes(hasher: &mut Sha256, value: Option<&[u8]>) {
    match value {
        Some(bytes) => {
            hasher.update([1]);
            update_bytes(hasher, bytes);
        }
        None => hasher.update([0]),
    }
}

fn update_text(hasher: &mut Sha256, value: &str) {
    update_bytes(hasher, value.as_bytes());
}

fn update_bytes(hasher: &mut Sha256, bytes: &[u8]) {
    update_count(hasher, bytes.len());
    hasher.update(bytes);
}

fn update_count(hasher: &mut Sha256, count: usize) {
    let count = u64::try_from(count).expect("cache key length fits the supported u64 domain");
    hasher.update(count.to_be_bytes());
}

fn update_physical_time(hasher: &mut Sha256, time: RationalTime) {
    let mut numerator = i128::from(time.value()) * i128::from(time.timebase().denominator());
    let mut denominator = i128::from(time.timebase().numerator());
    let divisor = greatest_common_divisor(numerator.abs(), denominator);
    numerator /= divisor;
    denominator /= divisor;
    hasher.update(numerator.to_be_bytes());
    hasher.update(denominator.to_be_bytes());
}

fn greatest_common_divisor(mut left: i128, mut right: i128) -> i128 {
    while right != 0 {
        let remainder = left % right;
        left = right;
        right = remainder;
    }
    left
}

fn write_digest(formatter: &mut fmt::Formatter<'_>, digest: &[u8; 32]) -> fmt::Result {
    formatter.write_str("sha256:")?;
    for byte in digest {
        write!(formatter, "{byte:02x}")?;
    }
    Ok(())
}
