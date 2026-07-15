//! Project media organization, metadata, and explicit relink state.

use std::collections::{BTreeMap, BTreeSet};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::{BinId, MediaId, SmartCollectionId};
use superi_core::time::TimeRange;

use crate::markers::{MetadataKey, MetadataValue, TimelineMetadata};

/// The deterministic metadata map attached to one linked media object.
pub type MediaMetadata = TimelineMetadata;

/// The editor-visible availability and verification state of linked media.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum RelinkStatus {
    /// The active target is usable and has no known identity conflict.
    Online,
    /// The active target is unavailable.
    Missing,
    /// The target changed without a confirming content fingerprint.
    Unverified,
    /// A candidate target was rejected because its content did not match.
    FingerprintMismatch,
}

/// The result of checking one relink candidate.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum RelinkDecision {
    /// The candidate became the active target.
    Accepted,
    /// The candidate was retained as evidence but did not replace the target.
    RejectedFingerprintMismatch,
}

/// Persistent, directly inspectable relink evidence for one media object.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaRelinkState {
    status: RelinkStatus,
    expected_fingerprint: Option<String>,
    observed_fingerprint: Option<String>,
    rejected_target: Option<String>,
}

impl MediaRelinkState {
    fn online_without_fingerprint() -> Self {
        Self {
            status: RelinkStatus::Online,
            expected_fingerprint: None,
            observed_fingerprint: None,
            rejected_target: None,
        }
    }

    fn verified(fingerprint: String) -> Self {
        Self {
            status: RelinkStatus::Online,
            expected_fingerprint: Some(fingerprint.clone()),
            observed_fingerprint: Some(fingerprint),
            rejected_target: None,
        }
    }

    /// Returns the current editor-visible relink status.
    #[must_use]
    pub const fn status(&self) -> RelinkStatus {
        self.status
    }

    /// Returns the content fingerprint that a relink must preserve, when known.
    #[must_use]
    pub fn expected_fingerprint(&self) -> Option<&str> {
        self.expected_fingerprint.as_deref()
    }

    /// Returns the most recently observed content fingerprint, when available.
    #[must_use]
    pub fn observed_fingerprint(&self) -> Option<&str> {
        self.observed_fingerprint.as_deref()
    }

    /// Returns the most recently rejected locator, when a mismatch occurred.
    #[must_use]
    pub fn rejected_target(&self) -> Option<&str> {
        self.rejected_target.as_deref()
    }

    fn validate(&self) -> Result<()> {
        if let Some(value) = &self.expected_fingerprint {
            require_text("validate_relink_state", "expected fingerprint", value)?;
        }
        if let Some(value) = &self.observed_fingerprint {
            require_text("validate_relink_state", "observed fingerprint", value)?;
        }
        if let Some(value) = &self.rejected_target {
            require_text("validate_relink_state", "rejected target", value)?;
        }
        if self.status == RelinkStatus::FingerprintMismatch {
            let expected = self.expected_fingerprint.as_deref().ok_or_else(|| {
                invalid(
                    "validate_relink_state",
                    "fingerprint mismatch state requires an expected fingerprint",
                )
            })?;
            let observed = self.observed_fingerprint.as_deref().ok_or_else(|| {
                invalid(
                    "validate_relink_state",
                    "fingerprint mismatch state requires an observed fingerprint",
                )
            })?;
            if expected == observed || self.rejected_target.is_none() {
                return Err(invalid(
                    "validate_relink_state",
                    "fingerprint mismatch state requires distinct evidence and a rejected target",
                ));
            }
        }
        Ok(())
    }
}

/// A media resource linked into an editorial project.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LinkedMediaReference {
    id: MediaId,
    name: String,
    target: String,
    available_range: Option<TimeRange>,
    metadata: MediaMetadata,
    relink_state: MediaRelinkState,
}

impl LinkedMediaReference {
    /// Creates a linked media reference without a known content fingerprint.
    #[must_use]
    pub fn new(
        id: MediaId,
        name: impl Into<String>,
        target: impl Into<String>,
        available_range: Option<TimeRange>,
    ) -> Self {
        Self {
            id,
            name: name.into(),
            target: target.into(),
            available_range,
            metadata: MediaMetadata::new(),
            relink_state: MediaRelinkState::online_without_fingerprint(),
        }
    }

    /// Creates a linked media reference with verified content identity.
    pub fn with_fingerprint(
        id: MediaId,
        name: impl Into<String>,
        target: impl Into<String>,
        available_range: Option<TimeRange>,
        fingerprint: impl Into<String>,
    ) -> Result<Self> {
        let fingerprint = fingerprint.into();
        require_text("create_media_reference", "media fingerprint", &fingerprint)?;
        let media = Self {
            id,
            name: name.into(),
            target: target.into(),
            available_range,
            metadata: MediaMetadata::new(),
            relink_state: MediaRelinkState::verified(fingerprint),
        };
        media.validate()?;
        Ok(media)
    }

    /// Returns the stable media identity.
    #[must_use]
    pub const fn id(&self) -> MediaId {
        self.id
    }

    /// Returns the editor-facing media name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the active opaque media locator.
    #[must_use]
    pub fn target(&self) -> &str {
        &self.target
    }

    /// Returns the known source availability, when discovery supplied one.
    #[must_use]
    pub const fn available_range(&self) -> Option<TimeRange> {
        self.available_range
    }

    /// Returns the deterministic media metadata.
    #[must_use]
    pub const fn metadata(&self) -> &MediaMetadata {
        &self.metadata
    }

    /// Mutably exposes metadata inside an unpublished project draft.
    pub fn metadata_mut(&mut self) -> &mut MediaMetadata {
        &mut self.metadata
    }

    /// Returns persistent relink status and identity evidence.
    #[must_use]
    pub const fn relink_state(&self) -> &MediaRelinkState {
        &self.relink_state
    }

    /// Replaces the editor-facing name inside an unpublished draft.
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    /// Replaces the locator without claiming that content identity was checked.
    pub fn set_target(&mut self, target: impl Into<String>) {
        self.target = target.into();
        self.relink_state.status = RelinkStatus::Unverified;
        self.relink_state.observed_fingerprint = None;
        self.relink_state.rejected_target = None;
    }

    /// Replaces the known available range inside an unpublished draft.
    pub fn set_available_range(&mut self, available_range: Option<TimeRange>) {
        self.available_range = available_range;
    }

    /// Marks the active locator unavailable without changing media identity.
    pub fn mark_missing(&mut self) {
        self.relink_state.status = RelinkStatus::Missing;
        self.relink_state.observed_fingerprint = None;
        self.relink_state.rejected_target = None;
    }

    /// Checks a candidate locator and publishes it only when identity matches.
    pub fn consider_relink(
        &mut self,
        candidate_target: impl Into<String>,
        observed_fingerprint: impl Into<String>,
    ) -> Result<RelinkDecision> {
        let candidate_target = candidate_target.into();
        let observed_fingerprint = observed_fingerprint.into();
        require_text("consider_relink", "candidate target", &candidate_target)?;
        require_text(
            "consider_relink",
            "observed fingerprint",
            &observed_fingerprint,
        )?;

        if let Some(expected) = self.relink_state.expected_fingerprint.as_deref() {
            if expected != observed_fingerprint {
                self.relink_state.status = RelinkStatus::FingerprintMismatch;
                self.relink_state.observed_fingerprint = Some(observed_fingerprint);
                self.relink_state.rejected_target = Some(candidate_target);
                return Ok(RelinkDecision::RejectedFingerprintMismatch);
            }
        }

        self.target = candidate_target;
        if self.relink_state.expected_fingerprint.is_none() {
            self.relink_state.expected_fingerprint = Some(observed_fingerprint.clone());
        }
        self.relink_state.status = RelinkStatus::Online;
        self.relink_state.observed_fingerprint = Some(observed_fingerprint);
        self.relink_state.rejected_target = None;
        Ok(RelinkDecision::Accepted)
    }

    pub(crate) fn validate(&self) -> Result<()> {
        require_text("validate_media", "media name", &self.name)?;
        require_text("validate_media", "media target", &self.target)?;
        self.relink_state.validate()
    }

    pub(crate) fn restore_relink_state(
        &mut self,
        status: RelinkStatus,
        expected_fingerprint: Option<String>,
        observed_fingerprint: Option<String>,
        rejected_target: Option<String>,
    ) -> Result<()> {
        let state = MediaRelinkState {
            status,
            expected_fingerprint,
            observed_fingerprint,
            rejected_target,
        };
        state.validate()?;
        self.relink_state = state;
        Ok(())
    }
}

/// A manually organized, optionally nested media container.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MediaBin {
    id: BinId,
    name: String,
    parent: Option<BinId>,
    media_ids: Vec<MediaId>,
}

impl MediaBin {
    /// Creates an empty bin with an optional parent.
    pub fn new(id: BinId, name: impl Into<String>, parent: Option<BinId>) -> Result<Self> {
        let name = name.into();
        require_text("create_bin", "bin name", &name)?;
        Ok(Self {
            id,
            name,
            parent,
            media_ids: Vec::new(),
        })
    }

    /// Returns the stable bin identity.
    #[must_use]
    pub const fn id(&self) -> BinId {
        self.id
    }

    /// Returns the editor-facing bin name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the parent bin, or `None` for a root bin.
    #[must_use]
    pub const fn parent(&self) -> Option<BinId> {
        self.parent
    }

    /// Returns manual membership in stable media identity order.
    #[must_use]
    pub fn media_ids(&self) -> &[MediaId] {
        &self.media_ids
    }

    /// Replaces the editor-facing bin name inside an unpublished draft.
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    /// Moves this bin under another bin or to the library root.
    pub fn set_parent(&mut self, parent: Option<BinId>) {
        self.parent = parent;
    }

    /// Adds one manual member while retaining stable order.
    pub fn add_media(&mut self, media_id: MediaId) -> bool {
        match self.media_ids.binary_search(&media_id) {
            Ok(_) => false,
            Err(index) => {
                self.media_ids.insert(index, media_id);
                true
            }
        }
    }

    /// Removes one manual member.
    pub fn remove_media(&mut self, media_id: MediaId) -> bool {
        let Ok(index) = self.media_ids.binary_search(&media_id) else {
            return false;
        };
        self.media_ids.remove(index);
        true
    }
}

/// How a smart collection combines its predicates.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum SmartCollectionMatch {
    /// Every predicate must match.
    All,
    /// At least one predicate must match.
    Any,
}

/// One directly inspectable condition in a saved media query.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum MediaPredicate {
    /// Match case-insensitive text within the editor-facing media name.
    NameContains(String),
    /// Match case-insensitive text within the active locator.
    TargetContains(String),
    /// Match media that contains one metadata key.
    MetadataExists(MetadataKey),
    /// Match one exact metadata value.
    MetadataEquals {
        /// Canonical metadata key.
        key: MetadataKey,
        /// Exact metadata value.
        value: MetadataValue,
    },
    /// Match one explicit relink state.
    RelinkStatus(RelinkStatus),
}

impl MediaPredicate {
    fn validate(&self) -> Result<()> {
        match self {
            Self::NameContains(value) | Self::TargetContains(value) => {
                require_text("validate_smart_collection", "query text", value)
            }
            _ => Ok(()),
        }
    }

    fn matches(&self, media: &LinkedMediaReference) -> bool {
        match self {
            Self::NameContains(value) => {
                media.name().to_lowercase().contains(&value.to_lowercase())
            }
            Self::TargetContains(value) => media
                .target()
                .to_lowercase()
                .contains(&value.to_lowercase()),
            Self::MetadataExists(key) => media.metadata().get(key).is_some(),
            Self::MetadataEquals { key, value } => media.metadata().get(key) == Some(value),
            Self::RelinkStatus(status) => media.relink_state().status() == *status,
        }
    }
}

/// A saved dynamic media query whose results are always derived on demand.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SmartCollection {
    id: SmartCollectionId,
    name: String,
    match_mode: SmartCollectionMatch,
    predicates: Vec<MediaPredicate>,
}

impl SmartCollection {
    /// Creates a smart collection with one or more editable predicates.
    pub fn new<I>(
        id: SmartCollectionId,
        name: impl Into<String>,
        match_mode: SmartCollectionMatch,
        predicates: I,
    ) -> Result<Self>
    where
        I: IntoIterator<Item = MediaPredicate>,
    {
        let collection = Self {
            id,
            name: name.into(),
            match_mode,
            predicates: predicates.into_iter().collect(),
        };
        collection.validate()?;
        Ok(collection)
    }

    /// Returns the stable smart collection identity.
    #[must_use]
    pub const fn id(&self) -> SmartCollectionId {
        self.id
    }

    /// Returns the editor-facing smart collection name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns how predicates are combined.
    #[must_use]
    pub const fn match_mode(&self) -> SmartCollectionMatch {
        self.match_mode
    }

    /// Returns the directly editable predicate list.
    #[must_use]
    pub fn predicates(&self) -> &[MediaPredicate] {
        &self.predicates
    }

    /// Replaces the editor-facing name inside an unpublished draft.
    pub fn set_name(&mut self, name: impl Into<String>) {
        self.name = name.into();
    }

    /// Replaces the predicate matching mode.
    pub fn set_match_mode(&mut self, match_mode: SmartCollectionMatch) {
        self.match_mode = match_mode;
    }

    /// Mutably exposes the predicate list inside an unpublished draft.
    pub fn predicates_mut(&mut self) -> &mut Vec<MediaPredicate> {
        &mut self.predicates
    }

    fn validate(&self) -> Result<()> {
        require_text(
            "validate_smart_collection",
            "smart collection name",
            &self.name,
        )?;
        if self.predicates.is_empty() {
            return Err(invalid(
                "validate_smart_collection",
                "smart collection must contain at least one predicate",
            ));
        }
        for predicate in &self.predicates {
            predicate.validate()?;
        }
        Ok(())
    }

    fn matches(&self, media: &LinkedMediaReference) -> bool {
        match self.match_mode {
            SmartCollectionMatch::All => self
                .predicates
                .iter()
                .all(|predicate| predicate.matches(media)),
            SmartCollectionMatch::Any => self
                .predicates
                .iter()
                .any(|predicate| predicate.matches(media)),
        }
    }
}

/// Project-owned manual organization and saved dynamic media queries.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct MediaLibrary {
    bins: BTreeMap<BinId, MediaBin>,
    smart_collections: BTreeMap<SmartCollectionId, SmartCollection>,
}

impl MediaLibrary {
    /// Creates an empty media library.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            bins: BTreeMap::new(),
            smart_collections: BTreeMap::new(),
        }
    }

    /// Looks up a bin by stable identity.
    #[must_use]
    pub fn bin(&self, id: BinId) -> Option<&MediaBin> {
        self.bins.get(&id)
    }

    /// Mutably looks up a bin inside an unpublished draft.
    pub fn bin_mut(&mut self, id: BinId) -> Result<&mut MediaBin> {
        self.bins
            .get_mut(&id)
            .ok_or_else(|| not_found("find_bin", "media bin was not found", "bin", id.to_string()))
    }

    /// Iterates all bins in stable identity order.
    pub fn bins(&self) -> impl ExactSizeIterator<Item = &MediaBin> {
        self.bins.values()
    }

    /// Iterates direct children of one parent in stable identity order.
    pub fn child_bins(&self, parent: Option<BinId>) -> impl Iterator<Item = &MediaBin> {
        self.bins.values().filter(move |bin| bin.parent() == parent)
    }

    /// Returns a bin's root-to-leaf identity path.
    pub fn bin_path(&self, id: BinId) -> Result<Vec<BinId>> {
        let mut path = Vec::new();
        let mut seen = BTreeSet::new();
        let mut current = Some(id);
        while let Some(bin_id) = current {
            if !seen.insert(bin_id) {
                return Err(conflict(
                    "resolve_bin_path",
                    "media bin nesting contains a cycle",
                    "bin",
                    bin_id.to_string(),
                ));
            }
            let bin = self.bins.get(&bin_id).ok_or_else(|| {
                not_found(
                    "resolve_bin_path",
                    "media bin parent was not found",
                    "bin",
                    bin_id.to_string(),
                )
            })?;
            path.push(bin_id);
            current = bin.parent();
        }
        path.reverse();
        Ok(path)
    }

    /// Inserts or replaces a bin by stable identity.
    pub fn upsert_bin(&mut self, bin: MediaBin) -> Option<MediaBin> {
        self.bins.insert(bin.id(), bin)
    }

    /// Removes a bin and its manual membership.
    pub fn remove_bin(&mut self, id: BinId) -> Option<MediaBin> {
        self.bins.remove(&id)
    }

    /// Moves media into one bin or removes it from manual bin membership.
    pub fn move_media(&mut self, media_id: MediaId, target: Option<BinId>) -> Result<()> {
        if let Some(target_id) = target {
            if !self.bins.contains_key(&target_id) {
                return Err(not_found(
                    "move_media",
                    "target media bin was not found",
                    "bin",
                    target_id.to_string(),
                ));
            }
        }
        for bin in self.bins.values_mut() {
            bin.remove_media(media_id);
        }
        if let Some(target_id) = target {
            self.bins
                .get_mut(&target_id)
                .expect("target bin checked")
                .add_media(media_id);
        }
        Ok(())
    }

    /// Looks up a smart collection by stable identity.
    #[must_use]
    pub fn smart_collection(&self, id: SmartCollectionId) -> Option<&SmartCollection> {
        self.smart_collections.get(&id)
    }

    /// Mutably looks up a smart collection inside an unpublished draft.
    pub fn smart_collection_mut(&mut self, id: SmartCollectionId) -> Result<&mut SmartCollection> {
        self.smart_collections.get_mut(&id).ok_or_else(|| {
            not_found(
                "find_smart_collection",
                "smart collection was not found",
                "smart_collection",
                id.to_string(),
            )
        })
    }

    /// Iterates smart collections in stable identity order.
    pub fn smart_collections(&self) -> impl ExactSizeIterator<Item = &SmartCollection> {
        self.smart_collections.values()
    }

    /// Inserts or replaces a smart collection by stable identity.
    pub fn upsert_smart_collection(
        &mut self,
        collection: SmartCollection,
    ) -> Option<SmartCollection> {
        self.smart_collections.insert(collection.id(), collection)
    }

    /// Removes a smart collection.
    pub fn remove_smart_collection(&mut self, id: SmartCollectionId) -> Option<SmartCollection> {
        self.smart_collections.remove(&id)
    }

    pub(crate) fn matching_media(
        &self,
        id: SmartCollectionId,
        media: &BTreeMap<MediaId, LinkedMediaReference>,
    ) -> Result<Vec<MediaId>> {
        let collection = self.smart_collections.get(&id).ok_or_else(|| {
            not_found(
                "evaluate_smart_collection",
                "smart collection was not found",
                "smart_collection",
                id.to_string(),
            )
        })?;
        Ok(media
            .iter()
            .filter_map(|(id, media)| collection.matches(media).then_some(*id))
            .collect())
    }

    pub(crate) fn validate(&self, media: &BTreeMap<MediaId, LinkedMediaReference>) -> Result<()> {
        for bin in self.bins.values() {
            require_text("validate_bin", "bin name", bin.name())?;
            if bin.parent() == Some(bin.id()) {
                return Err(conflict(
                    "validate_bin",
                    "media bin cannot be its own parent",
                    "bin",
                    bin.id().to_string(),
                ));
            }
            self.bin_path(bin.id())?;
        }

        let mut assigned = BTreeSet::new();
        for bin in self.bins.values() {
            for media_id in bin.media_ids() {
                if !media.contains_key(media_id) {
                    return Err(not_found(
                        "validate_bin",
                        "media bin references missing linked media",
                        "media",
                        media_id.to_string(),
                    ));
                }
                if !assigned.insert(*media_id) {
                    return Err(conflict(
                        "validate_bin",
                        "linked media belongs to more than one manual bin",
                        "media",
                        media_id.to_string(),
                    ));
                }
            }
        }
        for collection in self.smart_collections.values() {
            collection.validate()?;
        }
        Ok(())
    }
}

fn require_text(operation: &'static str, field: &'static str, value: &str) -> Result<()> {
    if value.trim().is_empty() || value.chars().any(char::is_control) {
        return Err(
            invalid(operation, "editorial text must be nonblank visible text").with_context(
                ErrorContext::new("superi-timeline.media", operation).with_field("field", field),
            ),
        );
    }
    Ok(())
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-timeline.media", operation))
}

fn not_found(
    operation: &'static str,
    message: &'static str,
    key: &'static str,
    value: String,
) -> Error {
    Error::new(
        ErrorCategory::NotFound,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-timeline.media", operation).with_field(key, value))
}

fn conflict(
    operation: &'static str,
    message: &'static str,
    key: &'static str,
    value: String,
) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new("superi-timeline.media", operation).with_field(key, value))
}
