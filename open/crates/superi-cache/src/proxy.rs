//! Media-neutral proxy and optimized-media generation identity and publication.
//!
//! This module intentionally does not depend on media I/O or codecs. The engine supplies complete
//! generated payloads through the fallible producer boundary, while this crate binds each result to
//! exact source identity, revision, purpose, quality, and render settings. Publication happens only
//! after generation succeeds, so a failure retains any previous complete artifact and otherwise
//! leaves the authoritative source as the explicit fallback.

use std::collections::BTreeMap;
use std::fmt;
use std::sync::Arc;

use sha2::{Digest, Sha256};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::CacheId;

use crate::key::{MediaCacheIdentity, RenderSettingsFingerprint};

const DERIVED_MEDIA_KEY_DOMAIN: &[u8] = b"superi.cache.derived-media-key.v1\0";
const DERIVED_MEDIA_ARTIFACT_DOMAIN: &[u8] = b"superi.cache.derived-media-artifact.v1\0";

/// The semantic role of one generated media artifact.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
#[non_exhaustive]
pub enum DerivedMediaPurpose {
    /// Replaceable lower-cost media intended for interactive work.
    Proxy = 0,
    /// Replaceable media optimized for a caller-declared processing path.
    Optimized = 1,
}

impl DerivedMediaPurpose {
    /// Returns the stable machine code used by identities and diagnostics.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Proxy => "proxy",
            Self::Optimized => "optimized",
        }
    }
}

/// One explicit generated-media quality choice.
///
/// These codes match the scheduler-facing quality vocabulary. The engine owns conversion between
/// scheduler requests and cache generation requests so this lower crate does not depend upward on
/// `superi-concurrency`.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[repr(u8)]
#[non_exhaustive]
pub enum DerivedMediaQuality {
    /// One-eighth linear resolution.
    Eighth = 0,
    /// One-quarter linear resolution.
    Quarter = 1,
    /// One-half linear resolution.
    Half = 2,
    /// Full declared resolution.
    Full = 3,
}

impl DerivedMediaQuality {
    /// All quality choices in ascending fidelity order.
    pub const ALL: &'static [Self] = &[Self::Eighth, Self::Quarter, Self::Half, Self::Full];

    /// Returns the stable machine code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Eighth => "eighth",
            Self::Quarter => "quarter",
            Self::Half => "half",
            Self::Full => "full",
        }
    }

    /// Returns the stable ascending quality rank.
    #[must_use]
    pub const fn rank(self) -> u8 {
        self as u8
    }
}

/// Complete immutable identity for one generation request.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DerivedMediaRequest {
    media: MediaCacheIdentity,
    source_revision: u64,
    purpose: DerivedMediaPurpose,
    quality: DerivedMediaQuality,
    render_settings: RenderSettingsFingerprint,
    key: DerivedMediaKey,
}

impl DerivedMediaRequest {
    /// Creates a request whose exact semantic inputs determine its reusable key.
    #[must_use]
    pub fn new(
        media: MediaCacheIdentity,
        source_revision: u64,
        purpose: DerivedMediaPurpose,
        quality: DerivedMediaQuality,
        render_settings: RenderSettingsFingerprint,
    ) -> Self {
        let key =
            DerivedMediaKey::derive(media, source_revision, purpose, quality, render_settings);
        Self {
            media,
            source_revision,
            purpose,
            quality,
            render_settings,
            key,
        }
    }

    /// Returns the persistent source identity and exact source-content digest.
    #[must_use]
    pub const fn media(self) -> MediaCacheIdentity {
        self.media
    }

    /// Returns the caller-owned source revision used for freshness.
    #[must_use]
    pub const fn source_revision(self) -> u64 {
        self.source_revision
    }

    /// Returns the generated artifact role.
    #[must_use]
    pub const fn purpose(self) -> DerivedMediaPurpose {
        self.purpose
    }

    /// Returns the exact quality choice.
    #[must_use]
    pub const fn quality(self) -> DerivedMediaQuality {
        self.quality
    }

    /// Returns the complete caller-owned render-setting identity.
    #[must_use]
    pub const fn render_settings(self) -> RenderSettingsFingerprint {
        self.render_settings
    }

    /// Returns the deterministic generation key.
    #[must_use]
    pub const fn key(self) -> DerivedMediaKey {
        self.key
    }
}

/// Versioned SHA-256 identity for one exact generation request.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct DerivedMediaKey([u8; 32]);

impl DerivedMediaKey {
    fn derive(
        media: MediaCacheIdentity,
        source_revision: u64,
        purpose: DerivedMediaPurpose,
        quality: DerivedMediaQuality,
        render_settings: RenderSettingsFingerprint,
    ) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(DERIVED_MEDIA_KEY_DOMAIN);
        hasher.update(media.media_id().to_bytes());
        hasher.update(media.content_fingerprint());
        hasher.update(source_revision.to_be_bytes());
        hasher.update([purpose as u8, quality as u8]);
        hasher.update(render_settings.as_bytes());
        Self(hasher.finalize().into())
    }

    /// Returns the exact digest bytes.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl fmt::Display for DerivedMediaKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("sha256:")?;
        for byte in self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// One complete generated payload before catalog publication.
#[derive(Debug)]
pub struct GeneratedMedia<T> {
    payload: T,
    content_fingerprint: [u8; 32],
    byte_len: u64,
}

impl<T> GeneratedMedia<T> {
    /// Creates a complete nonempty generated payload.
    pub fn new(payload: T, content_fingerprint: [u8; 32], byte_len: u64) -> Result<Self> {
        if byte_len == 0 {
            return Err(Error::new(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "generated media byte length must be greater than zero",
            )
            .with_context(ErrorContext::new(
                "superi-cache.proxy",
                "create_generated_media",
            )));
        }
        Ok(Self {
            payload,
            content_fingerprint,
            byte_len,
        })
    }
}

/// One immutable generated artifact bound to the request that produced it.
#[derive(Debug)]
pub struct DerivedMediaArtifact<T> {
    request: DerivedMediaRequest,
    cache_id: CacheId,
    content_fingerprint: [u8; 32],
    byte_len: u64,
    payload: T,
}

impl<T> DerivedMediaArtifact<T> {
    fn from_generated(request: DerivedMediaRequest, generated: GeneratedMedia<T>) -> Self {
        let cache_id = artifact_cache_id(request.key(), &generated.content_fingerprint);
        Self {
            request,
            cache_id,
            content_fingerprint: generated.content_fingerprint,
            byte_len: generated.byte_len,
            payload: generated.payload,
        }
    }

    /// Returns the complete generation request.
    #[must_use]
    pub const fn request(&self) -> DerivedMediaRequest {
        self.request
    }

    /// Returns a deterministic identifier for this exact generated content.
    #[must_use]
    pub const fn cache_id(&self) -> CacheId {
        self.cache_id
    }

    /// Returns the producer's exact generated-content digest.
    #[must_use]
    pub const fn content_fingerprint(&self) -> &[u8; 32] {
        &self.content_fingerprint
    }

    /// Returns the complete generated payload length.
    #[must_use]
    pub const fn byte_len(&self) -> u64 {
        self.byte_len
    }

    /// Borrows the immutable generated payload.
    #[must_use]
    pub const fn payload(&self) -> &T {
        &self.payload
    }

    /// Returns whether this artifact matches the current authoritative source state exactly.
    #[must_use]
    pub fn is_fresh(&self, media: MediaCacheIdentity, source_revision: u64) -> bool {
        self.request.media == media && self.request.source_revision == source_revision
    }
}

/// Exact lookup result for one requested derived artifact.
#[derive(Debug)]
pub enum DerivedMediaLookup<T> {
    /// A complete exact artifact is available.
    Generated(Arc<DerivedMediaArtifact<T>>),
    /// No exact artifact exists, so the authoritative original source remains the fallback.
    OriginalSource(MediaCacheIdentity),
}

/// In-memory publication catalog for complete replaceable generated artifacts.
#[derive(Debug)]
pub struct DerivedMediaCatalog<T> {
    artifacts: BTreeMap<DerivedMediaKey, Arc<DerivedMediaArtifact<T>>>,
}

/// Immutable ordered management view of published derived artifacts.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DerivedMediaCatalogInspection {
    keys: Vec<DerivedMediaKey>,
    entries: Vec<DerivedMediaInspectionEntry>,
    total_bytes: u128,
}

impl DerivedMediaCatalogInspection {
    /// Returns exact request keys in deterministic order.
    #[must_use]
    pub fn keys(&self) -> &[DerivedMediaKey] {
        &self.keys
    }

    /// Returns source, freshness, quality, content, and size evidence in key order.
    #[must_use]
    pub fn entries(&self) -> &[DerivedMediaInspectionEntry] {
        &self.entries
    }

    /// Returns the exact sum of producer-declared complete artifact bytes.
    #[must_use]
    pub const fn total_bytes(&self) -> u128 {
        self.total_bytes
    }
}

/// Complete payload-neutral identity and freshness evidence for one derived artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DerivedMediaInspectionEntry {
    request: DerivedMediaRequest,
    cache_id: CacheId,
    content_fingerprint: [u8; 32],
    byte_len: u64,
}

impl DerivedMediaInspectionEntry {
    fn from_artifact<T>(artifact: &DerivedMediaArtifact<T>) -> Self {
        Self {
            request: artifact.request(),
            cache_id: artifact.cache_id(),
            content_fingerprint: *artifact.content_fingerprint(),
            byte_len: artifact.byte_len(),
        }
    }

    /// Returns the complete source, revision, purpose, quality, and render-settings request.
    #[must_use]
    pub const fn request(&self) -> DerivedMediaRequest {
        self.request
    }

    /// Returns the stable identity of the exact published content.
    #[must_use]
    pub const fn cache_id(&self) -> CacheId {
        self.cache_id
    }

    /// Returns the producer's exact generated-content digest.
    #[must_use]
    pub const fn content_fingerprint(&self) -> &[u8; 32] {
        &self.content_fingerprint
    }

    /// Returns complete producer-declared bytes.
    #[must_use]
    pub const fn byte_len(&self) -> u64 {
        self.byte_len
    }
}

/// Deterministic evidence from removing every replaceable derived artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DerivedMediaCatalogClearReport {
    removed_keys: Vec<DerivedMediaKey>,
    removed_entries: Vec<DerivedMediaInspectionEntry>,
    removed_bytes: u128,
}

impl DerivedMediaCatalogClearReport {
    /// Returns removed request keys in deterministic order.
    #[must_use]
    pub fn removed_keys(&self) -> &[DerivedMediaKey] {
        &self.removed_keys
    }

    /// Returns complete payload-neutral evidence for every removed artifact.
    #[must_use]
    pub fn removed_entries(&self) -> &[DerivedMediaInspectionEntry] {
        &self.removed_entries
    }

    /// Returns the exact sum of removed producer-declared bytes.
    #[must_use]
    pub const fn removed_bytes(&self) -> u128 {
        self.removed_bytes
    }

    /// Returns whether the catalog was already empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.removed_keys.is_empty()
    }
}

impl<T> DerivedMediaCatalog<T> {
    /// Creates an empty catalog.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            artifacts: BTreeMap::new(),
        }
    }

    /// Returns the number of complete published artifacts.
    #[must_use]
    pub fn len(&self) -> usize {
        self.artifacts.len()
    }

    /// Returns whether no complete artifact has been published.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.artifacts.is_empty()
    }

    /// Captures exact request identities and complete artifact byte accounting.
    #[must_use]
    pub fn inspect(&self) -> DerivedMediaCatalogInspection {
        DerivedMediaCatalogInspection {
            keys: self.artifacts.keys().copied().collect(),
            entries: self
                .artifacts
                .values()
                .map(|artifact| DerivedMediaInspectionEntry::from_artifact(artifact))
                .collect(),
            total_bytes: self
                .artifacts
                .values()
                .map(|artifact| u128::from(artifact.byte_len()))
                .sum(),
        }
    }

    /// Removes every replaceable artifact without changing source identity or fallback policy.
    pub fn clear(&mut self) -> DerivedMediaCatalogClearReport {
        let artifacts = std::mem::take(&mut self.artifacts);
        let removed_keys = artifacts.keys().copied().collect();
        let removed_entries = artifacts
            .values()
            .map(|artifact| DerivedMediaInspectionEntry::from_artifact(artifact))
            .collect();
        let removed_bytes = artifacts
            .values()
            .map(|artifact| u128::from(artifact.byte_len()))
            .sum();
        drop(artifacts);
        DerivedMediaCatalogClearReport {
            removed_keys,
            removed_entries,
            removed_bytes,
        }
    }

    /// Resolves one exact request without selecting a different quality or stale revision.
    #[must_use]
    pub fn lookup(&self, request: DerivedMediaRequest) -> DerivedMediaLookup<T> {
        self.artifacts.get(&request.key()).map_or_else(
            || DerivedMediaLookup::OriginalSource(request.media()),
            |artifact| DerivedMediaLookup::Generated(Arc::clone(artifact)),
        )
    }

    /// Reuses an exact complete artifact or publishes a newly generated one after success.
    pub fn get_or_generate<F>(
        &mut self,
        request: DerivedMediaRequest,
        producer: F,
    ) -> Result<Arc<DerivedMediaArtifact<T>>>
    where
        F: FnOnce() -> Result<GeneratedMedia<T>>,
    {
        if let Some(artifact) = self.artifacts.get(&request.key()) {
            return Ok(Arc::clone(artifact));
        }
        self.regenerate(request, producer)
    }

    /// Generates a replacement off-catalog and publishes it only after complete success.
    ///
    /// If the producer fails, this method leaves every existing artifact unchanged.
    pub fn regenerate<F>(
        &mut self,
        request: DerivedMediaRequest,
        producer: F,
    ) -> Result<Arc<DerivedMediaArtifact<T>>>
    where
        F: FnOnce() -> Result<GeneratedMedia<T>>,
    {
        let generated = producer()?;
        let artifact = Arc::new(DerivedMediaArtifact::from_generated(request, generated));
        self.artifacts.insert(request.key(), Arc::clone(&artifact));
        Ok(artifact)
    }
}

impl<T> Default for DerivedMediaCatalog<T> {
    fn default() -> Self {
        Self::new()
    }
}

fn artifact_cache_id(key: DerivedMediaKey, content_fingerprint: &[u8; 32]) -> CacheId {
    let mut hasher = Sha256::new();
    hasher.update(DERIVED_MEDIA_ARTIFACT_DOMAIN);
    hasher.update(key.as_bytes());
    hasher.update(content_fingerprint);
    let digest: [u8; 32] = hasher.finalize().into();
    let mut identifier = [0; 16];
    identifier.copy_from_slice(&digest[..16]);
    CacheId::from_bytes(identifier)
}
