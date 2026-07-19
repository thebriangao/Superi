//! Atomic caption editing and durable caption presentation metadata.

use std::collections::BTreeSet;
use std::str::FromStr;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result, ResultExt};
use superi_core::ids::{CaptionId, ClipId, TimelineId, TrackId};

use crate::markers::{MetadataKey, MetadataOwner, MetadataValue, TimelineMetadata};
use crate::model::{
    Caption, EditorialObjectId, EditorialProject, LanguageTag, ProjectDraft, Timeline,
};

/// Canonical metadata key for the visible caption speaker.
pub const CAPTION_SPEAKER_METADATA_KEY: &str = "superi.caption.speaker";
/// Canonical metadata key for the complete caption presentation style.
pub const CAPTION_STYLE_METADATA_KEY: &str = "superi.caption.style";
/// Canonical metadata key for editable source timeline relationships.
pub const CAPTION_TIMELINE_RELATIONSHIPS_METADATA_KEY: &str =
    "superi.caption.timeline_relationships";

const MAX_CAPTION_NAME_BYTES: usize = 512;
const MAX_CAPTION_TEXT_BYTES: usize = 32 * 1024;
const MAX_SPEAKER_BYTES: usize = 512;
const MAX_FONT_FAMILY_BYTES: usize = 256;
const MAX_TIMELINE_RELATIONSHIPS: usize = 64;

/// Portable horizontal alignment for one caption.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum CaptionAlignment {
    Start,
    Center,
    End,
}

impl CaptionAlignment {
    /// Returns the permanent lowercase metadata code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Center => "center",
            Self::End => "end",
        }
    }

    fn from_code(code: &str) -> Option<Self> {
        match code {
            "start" => Some(Self::Start),
            "center" => Some(Self::Center),
            "end" => Some(Self::End),
            _ => None,
        }
    }
}

/// Portable vertical placement for one caption.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum CaptionPosition {
    Top,
    Bottom,
}

impl CaptionPosition {
    /// Returns the permanent lowercase metadata code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Top => "top",
            Self::Bottom => "bottom",
        }
    }

    fn from_code(code: &str) -> Option<Self> {
        match code {
            "top" => Some(Self::Top),
            "bottom" => Some(Self::Bottom),
            _ => None,
        }
    }
}

/// Complete portable caption presentation style.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptionStyle {
    font_family: Option<String>,
    font_size: Option<u16>,
    foreground: Option<String>,
    background: Option<String>,
    bold: bool,
    italic: bool,
    alignment: CaptionAlignment,
    position: CaptionPosition,
}

impl CaptionStyle {
    /// Creates one validated portable style.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        font_family: Option<String>,
        font_size: Option<u16>,
        foreground: Option<String>,
        background: Option<String>,
        bold: bool,
        italic: bool,
        alignment: CaptionAlignment,
        position: CaptionPosition,
    ) -> Result<Self> {
        if let Some(value) = &font_family {
            if value.trim().is_empty()
                || value.len() > MAX_FONT_FAMILY_BYTES
                || value.chars().any(char::is_control)
            {
                return Err(caption_error(
                    ErrorCategory::InvalidInput,
                    "create_caption_style",
                    "caption font family must be bounded visible text",
                ));
            }
        }
        if font_size.is_some_and(|value| !(8..=256).contains(&value)) {
            return Err(caption_error(
                ErrorCategory::InvalidInput,
                "create_caption_style",
                "caption font size must be between 8 and 256 points",
            ));
        }
        for color in [&foreground, &background].into_iter().flatten() {
            if !valid_color(color) {
                return Err(caption_error(
                    ErrorCategory::InvalidInput,
                    "create_caption_style",
                    "caption colors must use canonical #RRGGBBAA syntax",
                ));
            }
        }
        Ok(Self {
            font_family,
            font_size,
            foreground,
            background,
            bold,
            italic,
            alignment,
            position,
        })
    }

    #[must_use]
    pub fn font_family(&self) -> Option<&str> {
        self.font_family.as_deref()
    }

    #[must_use]
    pub const fn font_size(&self) -> Option<u16> {
        self.font_size
    }

    #[must_use]
    pub fn foreground(&self) -> Option<&str> {
        self.foreground.as_deref()
    }

    #[must_use]
    pub fn background(&self) -> Option<&str> {
        self.background.as_deref()
    }

    #[must_use]
    pub const fn bold(&self) -> bool {
        self.bold
    }

    #[must_use]
    pub const fn italic(&self) -> bool {
        self.italic
    }

    #[must_use]
    pub const fn alignment(&self) -> CaptionAlignment {
        self.alignment
    }

    #[must_use]
    pub const fn position(&self) -> CaptionPosition {
        self.position
    }
}

/// One editable relationship from a caption to a timeline and optional source clip.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct CaptionTimelineRelationship {
    timeline_id: TimelineId,
    clip_id: Option<ClipId>,
}

impl CaptionTimelineRelationship {
    #[must_use]
    pub const fn new(timeline_id: TimelineId, clip_id: Option<ClipId>) -> Self {
        Self {
            timeline_id,
            clip_id,
        }
    }

    #[must_use]
    pub const fn timeline_id(self) -> TimelineId {
        self.timeline_id
    }

    #[must_use]
    pub const fn clip_id(self) -> Option<ClipId> {
        self.clip_id
    }
}

/// Typed durable caption attributes decoded from canonical timeline metadata.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct CaptionAttributes {
    speaker: Option<String>,
    style: Option<CaptionStyle>,
    timeline_relationships: Vec<CaptionTimelineRelationship>,
}

impl CaptionAttributes {
    /// Decodes strict caption metadata after proving that the caption exists.
    pub fn from_timeline(timeline: &Timeline, caption_id: CaptionId) -> Result<Self> {
        find_caption_track(timeline, caption_id)?;
        let Some(metadata) = timeline.metadata(caption_owner(caption_id)) else {
            return Ok(Self::default());
        };
        let speaker = match metadata.get(&metadata_key(CAPTION_SPEAKER_METADATA_KEY)) {
            None => None,
            Some(MetadataValue::Text(value)) => {
                validate_speaker(value)?;
                Some(value.clone())
            }
            Some(_) => return Err(corrupt_caption_metadata("decode_caption_speaker")),
        };
        let style = match metadata.get(&metadata_key(CAPTION_STYLE_METADATA_KEY)) {
            None => None,
            Some(MetadataValue::Map(value)) => Some(decode_style(value)?),
            Some(_) => return Err(corrupt_caption_metadata("decode_caption_style")),
        };
        let timeline_relationships =
            match metadata.get(&metadata_key(CAPTION_TIMELINE_RELATIONSHIPS_METADATA_KEY)) {
                None => Vec::new(),
                Some(MetadataValue::List(values)) => decode_relationships(values)?,
                Some(_) => return Err(corrupt_caption_metadata("decode_caption_relationships")),
            };
        Ok(Self {
            speaker,
            style,
            timeline_relationships,
        })
    }

    #[must_use]
    pub fn speaker(&self) -> Option<&str> {
        self.speaker.as_deref()
    }

    #[must_use]
    pub const fn style(&self) -> Option<&CaptionStyle> {
        self.style.as_ref()
    }

    #[must_use]
    pub fn timeline_relationships(&self) -> &[CaptionTimelineRelationship] {
        &self.timeline_relationships
    }
}

/// One canonical authored caption mutation.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CaptionMutation {
    SetName {
        timeline_id: TimelineId,
        caption_id: CaptionId,
        name: String,
    },
    SetText {
        timeline_id: TimelineId,
        caption_id: CaptionId,
        text: String,
    },
    SetLanguage {
        timeline_id: TimelineId,
        caption_id: CaptionId,
        language: Option<String>,
    },
    SetSpeaker {
        timeline_id: TimelineId,
        caption_id: CaptionId,
        speaker: Option<String>,
    },
    SetStyle {
        timeline_id: TimelineId,
        caption_id: CaptionId,
        style: Option<CaptionStyle>,
    },
    SetTimelineRelationships {
        timeline_id: TimelineId,
        caption_id: CaptionId,
        relationships: Vec<CaptionTimelineRelationship>,
    },
}

impl CaptionMutation {
    #[must_use]
    pub const fn timeline_id(&self) -> TimelineId {
        match self {
            Self::SetName { timeline_id, .. }
            | Self::SetText { timeline_id, .. }
            | Self::SetLanguage { timeline_id, .. }
            | Self::SetSpeaker { timeline_id, .. }
            | Self::SetStyle { timeline_id, .. }
            | Self::SetTimelineRelationships { timeline_id, .. } => *timeline_id,
        }
    }

    #[must_use]
    pub const fn caption_id(&self) -> CaptionId {
        match self {
            Self::SetName { caption_id, .. }
            | Self::SetText { caption_id, .. }
            | Self::SetLanguage { caption_id, .. }
            | Self::SetSpeaker { caption_id, .. }
            | Self::SetStyle { caption_id, .. }
            | Self::SetTimelineRelationships { caption_id, .. } => *caption_id,
        }
    }

    const fn kind(&self) -> CaptionMutationKind {
        match self {
            Self::SetName { .. } => CaptionMutationKind::SetName,
            Self::SetText { .. } => CaptionMutationKind::SetText,
            Self::SetLanguage { .. } => CaptionMutationKind::SetLanguage,
            Self::SetSpeaker { .. } => CaptionMutationKind::SetSpeaker,
            Self::SetStyle { .. } => CaptionMutationKind::SetStyle,
            Self::SetTimelineRelationships { .. } => CaptionMutationKind::SetTimelineRelationships,
        }
    }
}

/// Stable category for one applied caption mutation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum CaptionMutationKind {
    SetName,
    SetText,
    SetLanguage,
    SetSpeaker,
    SetStyle,
    SetTimelineRelationships,
}

/// Semantic result for one mutation in an atomic batch.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CaptionMutationOutcome {
    timeline_id: TimelineId,
    caption_id: CaptionId,
    kind: CaptionMutationKind,
}

impl CaptionMutationOutcome {
    #[must_use]
    pub const fn timeline_id(self) -> TimelineId {
        self.timeline_id
    }

    #[must_use]
    pub const fn caption_id(self) -> CaptionId {
        self.caption_id
    }

    #[must_use]
    pub const fn kind(self) -> CaptionMutationKind {
        self.kind
    }
}

/// Results published together at one editorial project revision.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CaptionMutationBatchResult {
    revision: u64,
    outcomes: Vec<CaptionMutationOutcome>,
}

impl CaptionMutationBatchResult {
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    #[must_use]
    pub fn outcomes(&self) -> &[CaptionMutationOutcome] {
        &self.outcomes
    }
}

/// Applies a nonempty ordered caption mutation batch atomically.
pub fn apply_caption_mutation_batch(
    project: &mut EditorialProject,
    expected_revision: u64,
    mutations: &[CaptionMutation],
) -> Result<CaptionMutationBatchResult> {
    if mutations.is_empty() {
        return Err(caption_error(
            ErrorCategory::InvalidInput,
            "apply_caption_mutation_batch",
            "a caption mutation batch must contain at least one mutation",
        ));
    }
    let mut outcomes = Vec::with_capacity(mutations.len());
    project.edit(expected_revision, |draft| {
        for (index, mutation) in mutations.iter().enumerate() {
            apply_mutation(draft, mutation).with_error_context(
                ErrorContext::new("superi-timeline.caption-ops", "apply_caption_mutation")
                    .with_field("index", index.to_string())
                    .with_field("timeline", mutation.timeline_id().to_string())
                    .with_field("caption", mutation.caption_id().to_string()),
            )?;
            outcomes.push(CaptionMutationOutcome {
                timeline_id: mutation.timeline_id(),
                caption_id: mutation.caption_id(),
                kind: mutation.kind(),
            });
        }
        Ok(())
    })?;
    Ok(CaptionMutationBatchResult {
        revision: project.revision(),
        outcomes,
    })
}

fn apply_mutation(draft: &mut ProjectDraft, mutation: &CaptionMutation) -> Result<()> {
    match mutation {
        CaptionMutation::SetName {
            timeline_id,
            caption_id,
            name,
        } => {
            validate_name(name)?;
            caption_mut(draft.timeline_mut(*timeline_id)?, *caption_id)?.set_name(name.clone());
        }
        CaptionMutation::SetText {
            timeline_id,
            caption_id,
            text,
        } => {
            validate_text(text)?;
            caption_mut(draft.timeline_mut(*timeline_id)?, *caption_id)?.set_text(text.clone());
        }
        CaptionMutation::SetLanguage {
            timeline_id,
            caption_id,
            language,
        } => {
            if let Some(value) = language {
                LanguageTag::new(value.clone())?;
            }
            caption_mut(draft.timeline_mut(*timeline_id)?, *caption_id)?
                .set_language(language.clone());
        }
        CaptionMutation::SetSpeaker {
            timeline_id,
            caption_id,
            speaker,
        } => {
            if let Some(value) = speaker {
                validate_speaker(value)?;
            }
            update_caption_metadata(draft.timeline_mut(*timeline_id)?, *caption_id, |metadata| {
                replace_optional_metadata(
                    metadata,
                    CAPTION_SPEAKER_METADATA_KEY,
                    speaker.clone().map(MetadataValue::Text),
                );
                Ok(())
            })?;
        }
        CaptionMutation::SetStyle {
            timeline_id,
            caption_id,
            style,
        } => {
            update_caption_metadata(draft.timeline_mut(*timeline_id)?, *caption_id, |metadata| {
                replace_optional_metadata(
                    metadata,
                    CAPTION_STYLE_METADATA_KEY,
                    style
                        .as_ref()
                        .map(|value| MetadataValue::Map(encode_style(value))),
                );
                Ok(())
            })?;
        }
        CaptionMutation::SetTimelineRelationships {
            timeline_id,
            caption_id,
            relationships,
        } => {
            let relationships = validate_relationships(draft, relationships)?;
            update_caption_metadata(draft.timeline_mut(*timeline_id)?, *caption_id, |metadata| {
                let value = (!relationships.is_empty()).then(|| {
                    MetadataValue::List(
                        relationships
                            .iter()
                            .copied()
                            .map(encode_relationship)
                            .collect(),
                    )
                });
                replace_optional_metadata(
                    metadata,
                    CAPTION_TIMELINE_RELATIONSHIPS_METADATA_KEY,
                    value,
                );
                Ok(())
            })?;
        }
    }
    Ok(())
}

fn caption_mut(timeline: &mut Timeline, caption_id: CaptionId) -> Result<&mut Caption> {
    let track_id = find_caption_track(timeline, caption_id)?;
    timeline
        .track_mut(track_id)?
        .item_mut(EditorialObjectId::Caption(caption_id))?
        .as_caption_mut()
        .ok_or_else(|| caption_not_found(caption_id))
}

fn find_caption_track(timeline: &Timeline, caption_id: CaptionId) -> Result<TrackId> {
    let object = EditorialObjectId::Caption(caption_id);
    timeline
        .tracks()
        .iter()
        .find(|track| {
            track
                .item(object)
                .and_then(|item| item.as_caption())
                .is_some()
        })
        .map(|track| track.id())
        .ok_or_else(|| caption_not_found(caption_id))
}

fn update_caption_metadata(
    timeline: &mut Timeline,
    caption_id: CaptionId,
    update: impl FnOnce(&mut TimelineMetadata) -> Result<()>,
) -> Result<()> {
    find_caption_track(timeline, caption_id)?;
    let owner = caption_owner(caption_id);
    let mut metadata = timeline.metadata(owner).cloned().unwrap_or_default();
    update(&mut metadata)?;
    if metadata.is_empty() {
        timeline.remove_metadata(owner);
    } else {
        timeline.set_metadata(owner, metadata)?;
    }
    Ok(())
}

fn validate_relationships(
    draft: &ProjectDraft,
    relationships: &[CaptionTimelineRelationship],
) -> Result<Vec<CaptionTimelineRelationship>> {
    if relationships.len() > MAX_TIMELINE_RELATIONSHIPS {
        return Err(caption_error(
            ErrorCategory::ResourceExhausted,
            "validate_caption_relationships",
            "caption timeline relationships exceed the supported bound",
        ));
    }
    let mut canonical = BTreeSet::new();
    for relationship in relationships {
        let timeline = draft.timeline(relationship.timeline_id()).ok_or_else(|| {
            caption_error(
                ErrorCategory::NotFound,
                "validate_caption_relationships",
                "caption relationship timeline was not found",
            )
        })?;
        if let Some(clip_id) = relationship.clip_id() {
            let found = timeline
                .tracks()
                .iter()
                .any(|track| track.item(EditorialObjectId::Clip(clip_id)).is_some());
            if !found {
                return Err(caption_error(
                    ErrorCategory::NotFound,
                    "validate_caption_relationships",
                    "caption relationship clip was not found on its timeline",
                ));
            }
        }
        if !canonical.insert(*relationship) {
            return Err(caption_error(
                ErrorCategory::InvalidInput,
                "validate_caption_relationships",
                "caption timeline relationships must not contain duplicates",
            ));
        }
    }
    Ok(canonical.into_iter().collect())
}

fn encode_style(style: &CaptionStyle) -> TimelineMetadata {
    TimelineMetadata::from_entries([
        (
            metadata_key("font_family"),
            style.font_family().map_or(MetadataValue::Null, |value| {
                MetadataValue::Text(value.to_owned())
            }),
        ),
        (
            metadata_key("font_size"),
            style.font_size().map_or(MetadataValue::Null, |value| {
                MetadataValue::Unsigned(u64::from(value))
            }),
        ),
        (
            metadata_key("foreground"),
            style.foreground().map_or(MetadataValue::Null, |value| {
                MetadataValue::Text(value.to_owned())
            }),
        ),
        (
            metadata_key("background"),
            style.background().map_or(MetadataValue::Null, |value| {
                MetadataValue::Text(value.to_owned())
            }),
        ),
        (metadata_key("bold"), MetadataValue::Boolean(style.bold())),
        (
            metadata_key("italic"),
            MetadataValue::Boolean(style.italic()),
        ),
        (
            metadata_key("alignment"),
            MetadataValue::Text(style.alignment().code().to_owned()),
        ),
        (
            metadata_key("position"),
            MetadataValue::Text(style.position().code().to_owned()),
        ),
    ])
}

fn decode_style(metadata: &TimelineMetadata) -> Result<CaptionStyle> {
    let optional_text = |key: &str| -> Result<Option<String>> {
        match metadata.get(&metadata_key(key)) {
            Some(MetadataValue::Null) | None => Ok(None),
            Some(MetadataValue::Text(value)) => Ok(Some(value.clone())),
            Some(_) => Err(corrupt_caption_metadata("decode_caption_style")),
        }
    };
    let font_size = match metadata.get(&metadata_key("font_size")) {
        Some(MetadataValue::Null) | None => None,
        Some(MetadataValue::Unsigned(value)) => Some(
            u16::try_from(*value).map_err(|_| corrupt_caption_metadata("decode_caption_style"))?,
        ),
        Some(_) => return Err(corrupt_caption_metadata("decode_caption_style")),
    };
    let boolean = |key: &str| -> Result<bool> {
        match metadata.get(&metadata_key(key)) {
            Some(MetadataValue::Boolean(value)) => Ok(*value),
            _ => Err(corrupt_caption_metadata("decode_caption_style")),
        }
    };
    let code = |key: &str| -> Result<&str> {
        match metadata.get(&metadata_key(key)) {
            Some(MetadataValue::Text(value)) => Ok(value.as_str()),
            _ => Err(corrupt_caption_metadata("decode_caption_style")),
        }
    };
    let alignment = CaptionAlignment::from_code(code("alignment")?)
        .ok_or_else(|| corrupt_caption_metadata("decode_caption_style"))?;
    let position = CaptionPosition::from_code(code("position")?)
        .ok_or_else(|| corrupt_caption_metadata("decode_caption_style"))?;
    CaptionStyle::new(
        optional_text("font_family")?,
        font_size,
        optional_text("foreground")?,
        optional_text("background")?,
        boolean("bold")?,
        boolean("italic")?,
        alignment,
        position,
    )
}

fn encode_relationship(relationship: CaptionTimelineRelationship) -> MetadataValue {
    MetadataValue::Map(TimelineMetadata::from_entries([
        (
            metadata_key("timeline_id"),
            MetadataValue::Text(relationship.timeline_id().to_string()),
        ),
        (
            metadata_key("clip_id"),
            relationship.clip_id().map_or(MetadataValue::Null, |id| {
                MetadataValue::Text(id.to_string())
            }),
        ),
    ]))
}

fn decode_relationships(values: &[MetadataValue]) -> Result<Vec<CaptionTimelineRelationship>> {
    if values.len() > MAX_TIMELINE_RELATIONSHIPS {
        return Err(corrupt_caption_metadata("decode_caption_relationships"));
    }
    let mut relationships = BTreeSet::new();
    for value in values {
        let MetadataValue::Map(value) = value else {
            return Err(corrupt_caption_metadata("decode_caption_relationships"));
        };
        let timeline_id = match value.get(&metadata_key("timeline_id")) {
            Some(MetadataValue::Text(value)) => TimelineId::from_str(value)
                .map_err(|_| corrupt_caption_metadata("decode_caption_relationships"))?,
            _ => return Err(corrupt_caption_metadata("decode_caption_relationships")),
        };
        let clip_id = match value.get(&metadata_key("clip_id")) {
            Some(MetadataValue::Null) | None => None,
            Some(MetadataValue::Text(value)) => Some(
                ClipId::from_str(value)
                    .map_err(|_| corrupt_caption_metadata("decode_caption_relationships"))?,
            ),
            _ => return Err(corrupt_caption_metadata("decode_caption_relationships")),
        };
        if !relationships.insert(CaptionTimelineRelationship::new(timeline_id, clip_id)) {
            return Err(corrupt_caption_metadata("decode_caption_relationships"));
        }
    }
    Ok(relationships.into_iter().collect())
}

fn replace_optional_metadata(
    metadata: &mut TimelineMetadata,
    key: &str,
    value: Option<MetadataValue>,
) {
    let key = metadata_key(key);
    if let Some(value) = value {
        metadata.insert(key, value);
    } else {
        metadata.remove(&key);
    }
}

fn validate_name(value: &str) -> Result<()> {
    if value.trim().is_empty()
        || value.len() > MAX_CAPTION_NAME_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(caption_error(
            ErrorCategory::InvalidInput,
            "validate_caption_name",
            "caption name must be bounded visible text",
        ));
    }
    Ok(())
}

fn validate_text(value: &str) -> Result<()> {
    if value.trim().is_empty() || value.len() > MAX_CAPTION_TEXT_BYTES || value.contains('\0') {
        return Err(caption_error(
            ErrorCategory::InvalidInput,
            "validate_caption_text",
            "caption text must be bounded nonblank text without null bytes",
        ));
    }
    Ok(())
}

fn validate_speaker(value: &str) -> Result<()> {
    if value.trim().is_empty()
        || value.len() > MAX_SPEAKER_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(caption_error(
            ErrorCategory::InvalidInput,
            "validate_caption_speaker",
            "caption speaker must be bounded visible text",
        ));
    }
    Ok(())
}

fn valid_color(value: &str) -> bool {
    value.len() == 9
        && value.starts_with('#')
        && value[1..].bytes().all(|byte| byte.is_ascii_hexdigit())
        && value[1..].bytes().all(|byte| !byte.is_ascii_uppercase())
}

fn caption_owner(caption_id: CaptionId) -> MetadataOwner {
    MetadataOwner::Object(EditorialObjectId::Caption(caption_id))
}

fn metadata_key(value: &str) -> MetadataKey {
    MetadataKey::new(value).expect("static caption metadata key is canonical")
}

fn caption_not_found(caption_id: CaptionId) -> Error {
    caption_error(
        ErrorCategory::NotFound,
        "find_caption",
        "editorial caption was not found",
    )
    .with_context(
        ErrorContext::new("superi-timeline.caption-ops", "find_caption")
            .with_field("caption", caption_id.to_string()),
    )
}

fn corrupt_caption_metadata(operation: &'static str) -> Error {
    caption_error(
        ErrorCategory::CorruptData,
        operation,
        "caption metadata does not match the canonical typed schema",
    )
}

fn caption_error(category: ErrorCategory, operation: &'static str, message: &'static str) -> Error {
    Error::new(category, Recoverability::UserCorrectable, message)
        .with_context(ErrorContext::new("superi-timeline.caption-ops", operation))
}
