//! Editable text authoring, real font shaping, and inspectable paragraph layout.
//!
//! This module owns text content, typography, paragraph controls, exact animation, strict
//! persistence, and a bounded CPU layout result. Font discovery remains caller-owned and offline.
//! Glyph rasterization, atlases, GPU resources, engine registration, and UI remain separate owners.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::Arc;

use serde::de::{Error as _, IgnoredAny, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::time::{RationalTime, TimeRounding, Timebase};
use swash::shape::{Direction as ShapeDirection, ShapeContext};
use swash::text::{Codepoint, Script};
use swash::FontRef;
use unicode_bidi::{BidiInfo, Level};
use unicode_linebreak::{linebreaks, BreakOpportunity};

use crate::keyframe::{AnimationCurve, Interpolation};

const COMPONENT: &str = "superi-effects::text";

/// Current standalone text layer wire revision.
pub const TEXT_LAYER_SCHEMA_REVISION: u32 = 1;
/// Maximum UTF-8 bytes in one editable layer.
pub const MAX_TEXT_BYTES: usize = 1_048_576;
/// Maximum style or paragraph spans in one layer.
pub const MAX_TEXT_SPANS: usize = 65_536;
/// Maximum OpenType features or variable axes on one style.
pub const MAX_FONT_SETTINGS: usize = 256;
/// Maximum positioned glyphs in one layout result.
pub const MAX_LAYOUT_GLYPHS: usize = 1_048_576;

/// A half-open UTF-8 byte range into the owning text.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd, Serialize)]
pub struct TextRange {
    start: usize,
    end: usize,
}

impl TextRange {
    /// Creates an ordered half-open range. UTF-8 boundaries are checked by the owning layer.
    pub fn new(start: usize, end: usize) -> Result<Self> {
        if end < start {
            return Err(text_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "create_range",
                "range_not_ordered",
                "text range end must not precede its start",
            ));
        }
        Ok(Self { start, end })
    }

    /// Returns the inclusive byte start.
    #[must_use]
    pub const fn start(self) -> usize {
        self.start
    }

    /// Returns the exclusive byte end.
    #[must_use]
    pub const fn end(self) -> usize {
        self.end
    }

    /// Returns the byte length.
    #[must_use]
    pub const fn len(self) -> usize {
        self.end - self.start
    }

    /// Returns whether the range is empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.start == self.end
    }

    fn contains_range(self, other: Self) -> bool {
        self.start <= other.start && other.end <= self.end
    }
}

/// Stable caller-owned reference to one font face in an offline asset.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct FontFace {
    asset_id: String,
    family: String,
    display_name: String,
    collection_index: u32,
}

impl FontFace {
    /// Creates one checked persistent font reference without consulting the host font database.
    pub fn new(
        asset_id: impl Into<String>,
        family: impl Into<String>,
        display_name: impl Into<String>,
        collection_index: u32,
    ) -> Result<Self> {
        let face = Self {
            asset_id: asset_id.into(),
            family: family.into(),
            display_name: display_name.into(),
            collection_index,
        };
        validate_label(&face.asset_id, 256, "asset_id", "create_font_face")?;
        validate_label(&face.family, 256, "family", "create_font_face")?;
        validate_label(&face.display_name, 256, "display_name", "create_font_face")?;
        Ok(face)
    }

    /// Returns the stable caller asset identity.
    #[must_use]
    pub fn asset_id(&self) -> &str {
        &self.asset_id
    }

    /// Returns the inspectable font family.
    #[must_use]
    pub fn family(&self) -> &str {
        &self.family
    }

    /// Returns the inspectable face label.
    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    /// Returns the face index inside a font collection.
    #[must_use]
    pub const fn collection_index(&self) -> u32 {
        self.collection_index
    }
}

/// One explicit OpenType feature setting.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
pub struct OpenTypeFeature {
    tag: [u8; 4],
    value: u16,
}

impl OpenTypeFeature {
    /// Creates a setting with one printable four-byte OpenType tag.
    pub fn new(tag: [u8; 4], value: u16) -> Result<Self> {
        validate_tag(tag, "create_feature")?;
        Ok(Self { tag, value })
    }

    /// Returns the exact four-byte tag.
    #[must_use]
    pub const fn tag(self) -> [u8; 4] {
        self.tag
    }

    /// Returns the feature selector value.
    #[must_use]
    pub const fn value(self) -> u16 {
        self.value
    }
}

/// One animated OpenType variable-font axis.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct VariationAxis {
    tag: [u8; 4],
    value: AnimationCurve,
}

impl VariationAxis {
    /// Creates one scalar finite animated axis setting.
    pub fn new(tag: [u8; 4], value: AnimationCurve) -> Result<Self> {
        validate_tag(tag, "create_variation_axis")?;
        validate_curve(&value, 1, CurveDomain::Finite, "variation_axis")?;
        Ok(Self { tag, value })
    }

    /// Returns the exact axis tag.
    #[must_use]
    pub const fn tag(&self) -> [u8; 4] {
        self.tag
    }

    /// Returns the directly editable axis curve.
    #[must_use]
    pub const fn value(&self) -> &AnimationCurve {
        &self.value
    }

    fn retimed(&self, start: RationalTime, end: RationalTime) -> Result<Self> {
        Self::new(self.tag, self.value.retimed(start, end)?)
    }
}

/// Complete directly editable typography for one text range.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TextStyle {
    font: FontFace,
    font_size: AnimationCurve,
    fill_rgba: AnimationCurve,
    opacity: AnimationCurve,
    tracking: AnimationCurve,
    baseline_shift: AnimationCurve,
    features: Vec<OpenTypeFeature>,
    variations: Vec<VariationAxis>,
}

impl TextStyle {
    /// Creates checked animated typography.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        font: FontFace,
        font_size: AnimationCurve,
        fill_rgba: AnimationCurve,
        opacity: AnimationCurve,
        tracking: AnimationCurve,
        baseline_shift: AnimationCurve,
        features: impl IntoIterator<Item = OpenTypeFeature>,
        variations: impl IntoIterator<Item = VariationAxis>,
    ) -> Result<Self> {
        let style = Self {
            font,
            font_size,
            fill_rgba,
            opacity,
            tracking,
            baseline_shift,
            features: collect_bounded(features, MAX_FONT_SETTINGS, "feature_limit")?,
            variations: collect_bounded(variations, MAX_FONT_SETTINGS, "variation_limit")?,
        };
        style.validate()?;
        Ok(style)
    }

    /// Returns the persistent face reference.
    #[must_use]
    pub const fn font(&self) -> &FontFace {
        &self.font
    }

    /// Returns the animated size in pixels per em.
    #[must_use]
    pub const fn font_size(&self) -> &AnimationCurve {
        &self.font_size
    }

    /// Returns the animated RGBA fill.
    #[must_use]
    pub const fn fill_rgba(&self) -> &AnimationCurve {
        &self.fill_rgba
    }

    /// Returns the animated normalized opacity.
    #[must_use]
    pub const fn opacity(&self) -> &AnimationCurve {
        &self.opacity
    }

    /// Returns the animated extra cluster advance in pixels.
    #[must_use]
    pub const fn tracking(&self) -> &AnimationCurve {
        &self.tracking
    }

    /// Returns the animated baseline displacement in pixels.
    #[must_use]
    pub const fn baseline_shift(&self) -> &AnimationCurve {
        &self.baseline_shift
    }

    /// Returns canonical feature settings.
    #[must_use]
    pub fn features(&self) -> &[OpenTypeFeature] {
        &self.features
    }

    /// Returns canonical variable-axis settings.
    #[must_use]
    pub fn variations(&self) -> &[VariationAxis] {
        &self.variations
    }

    fn validate(&self) -> Result<()> {
        validate_curve(&self.font_size, 1, CurveDomain::Positive, "font_size")?;
        validate_curve(&self.fill_rgba, 4, CurveDomain::Normalized, "fill_rgba")?;
        validate_curve(&self.opacity, 1, CurveDomain::Normalized, "opacity")?;
        validate_curve(&self.tracking, 1, CurveDomain::Finite, "tracking")?;
        validate_curve(
            &self.baseline_shift,
            1,
            CurveDomain::Finite,
            "baseline_shift",
        )?;
        let mut feature_tags = BTreeSet::new();
        for feature in &self.features {
            validate_tag(feature.tag, "validate_style")?;
            if !feature_tags.insert(feature.tag) {
                return Err(duplicate_setting("feature"));
            }
        }
        let mut axis_tags = BTreeSet::new();
        for axis in &self.variations {
            validate_tag(axis.tag, "validate_style")?;
            validate_curve(&axis.value, 1, CurveDomain::Finite, "variation_axis")?;
            if !axis_tags.insert(axis.tag) {
                return Err(duplicate_setting("variation_axis"));
            }
        }
        validate_common_curves(self.curves())
    }

    fn curves(&self) -> Vec<&AnimationCurve> {
        let mut curves = vec![
            &self.font_size,
            &self.fill_rgba,
            &self.opacity,
            &self.tracking,
            &self.baseline_shift,
        ];
        curves.extend(self.variations.iter().map(|axis| &axis.value));
        curves
    }

    fn retimed(&self, start: RationalTime, end: RationalTime) -> Result<Self> {
        Self::new(
            self.font.clone(),
            self.font_size.retimed(start, end)?,
            self.fill_rgba.retimed(start, end)?,
            self.opacity.retimed(start, end)?,
            self.tracking.retimed(start, end)?,
            self.baseline_shift.retimed(start, end)?,
            self.features.clone(),
            self.variations
                .iter()
                .map(|axis| axis.retimed(start, end))
                .collect::<Result<Vec<_>>>()?,
        )
    }
}

/// One style over an exact text range.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct TextStyleSpan {
    range: TextRange,
    style: TextStyle,
}

impl TextStyleSpan {
    /// Creates a nonempty style span. Layer construction checks UTF-8 and coverage.
    pub fn new(range: TextRange, style: TextStyle) -> Result<Self> {
        if range.is_empty() {
            return Err(empty_span("style"));
        }
        Ok(Self { range, style })
    }

    /// Returns its half-open UTF-8 byte range.
    #[must_use]
    pub const fn range(&self) -> TextRange {
        self.range
    }

    /// Returns complete animated typography.
    #[must_use]
    pub const fn style(&self) -> &TextStyle {
        &self.style
    }
}

/// Paragraph alignment resolved at layout time.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextAlignment {
    /// Align to the resolved paragraph start edge.
    Start,
    /// Center within the paragraph measure.
    Center,
    /// Align to the resolved paragraph end edge.
    End,
    /// Expand eligible spaces on every nonfinal wrapped line.
    Justify,
}

impl TextAlignment {
    /// Returns the exact scalar code stored in discrete animation curves.
    #[must_use]
    pub const fn code(self) -> f64 {
        match self {
            Self::Start => 0.0,
            Self::Center => 1.0,
            Self::End => 2.0,
            Self::Justify => 3.0,
        }
    }

    fn from_code(value: f64) -> Option<Self> {
        match value {
            0.0 => Some(Self::Start),
            1.0 => Some(Self::Center),
            2.0 => Some(Self::End),
            3.0 => Some(Self::Justify),
            _ => None,
        }
    }
}

/// Authored or resolved paragraph direction.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextDirection {
    /// Resolve the base level from Unicode content.
    Auto,
    /// Force a left-to-right paragraph or identify an LTR visual run.
    LeftToRight,
    /// Force a right-to-left paragraph or identify an RTL visual run.
    RightToLeft,
}

impl TextDirection {
    /// Returns the exact scalar code stored in discrete animation curves.
    #[must_use]
    pub const fn code(self) -> f64 {
        match self {
            Self::Auto => 0.0,
            Self::LeftToRight => 1.0,
            Self::RightToLeft => 2.0,
        }
    }

    fn from_code(value: f64) -> Option<Self> {
        match value {
            0.0 => Some(Self::Auto),
            1.0 => Some(Self::LeftToRight),
            2.0 => Some(Self::RightToLeft),
            _ => None,
        }
    }
}

/// Authored line wrapping behavior.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TextWrap {
    /// Wrap at Unicode line-break opportunities, allowing an overlong unbreakable word.
    Word,
    /// Prefer Unicode opportunities and fall back to shaped cluster boundaries.
    Anywhere,
    /// Preserve one visual line per paragraph.
    NoWrap,
}

impl TextWrap {
    /// Returns the exact scalar code stored in discrete animation curves.
    #[must_use]
    pub const fn code(self) -> f64 {
        match self {
            Self::Word => 0.0,
            Self::Anywhere => 1.0,
            Self::NoWrap => 2.0,
        }
    }

    fn from_code(value: f64) -> Option<Self> {
        match value {
            0.0 => Some(Self::Word),
            1.0 => Some(Self::Anywhere),
            2.0 => Some(Self::NoWrap),
            _ => None,
        }
    }
}

/// Complete animated layout controls for one paragraph.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ParagraphStyle {
    width: AnimationCurve,
    line_height: AnimationCurve,
    first_line_indent: AnimationCurve,
    start_indent: AnimationCurve,
    end_indent: AnimationCurve,
    space_before: AnimationCurve,
    space_after: AnimationCurve,
    alignment: AnimationCurve,
    direction: AnimationCurve,
    wrap: AnimationCurve,
}

impl ParagraphStyle {
    /// Creates checked continuous and hold-discrete paragraph controls.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        width: AnimationCurve,
        line_height: AnimationCurve,
        first_line_indent: AnimationCurve,
        start_indent: AnimationCurve,
        end_indent: AnimationCurve,
        space_before: AnimationCurve,
        space_after: AnimationCurve,
        alignment: AnimationCurve,
        direction: AnimationCurve,
        wrap: AnimationCurve,
    ) -> Result<Self> {
        let style = Self {
            width,
            line_height,
            first_line_indent,
            start_indent,
            end_indent,
            space_before,
            space_after,
            alignment,
            direction,
            wrap,
        };
        style.validate()?;
        Ok(style)
    }

    /// Returns the animated paragraph width.
    #[must_use]
    pub const fn width(&self) -> &AnimationCurve {
        &self.width
    }

    /// Returns the animated baseline-to-baseline distance.
    #[must_use]
    pub const fn line_height(&self) -> &AnimationCurve {
        &self.line_height
    }

    /// Returns the animated first-line indent.
    #[must_use]
    pub const fn first_line_indent(&self) -> &AnimationCurve {
        &self.first_line_indent
    }

    /// Returns the animated start indent.
    #[must_use]
    pub const fn start_indent(&self) -> &AnimationCurve {
        &self.start_indent
    }

    /// Returns the animated end indent.
    #[must_use]
    pub const fn end_indent(&self) -> &AnimationCurve {
        &self.end_indent
    }

    /// Returns animated spacing before the paragraph.
    #[must_use]
    pub const fn space_before(&self) -> &AnimationCurve {
        &self.space_before
    }

    /// Returns animated spacing after the paragraph.
    #[must_use]
    pub const fn space_after(&self) -> &AnimationCurve {
        &self.space_after
    }

    /// Returns the hold-discrete alignment curve.
    #[must_use]
    pub const fn alignment(&self) -> &AnimationCurve {
        &self.alignment
    }

    /// Returns the hold-discrete direction curve.
    #[must_use]
    pub const fn direction(&self) -> &AnimationCurve {
        &self.direction
    }

    /// Returns the hold-discrete wrap curve.
    #[must_use]
    pub const fn wrap(&self) -> &AnimationCurve {
        &self.wrap
    }

    fn validate(&self) -> Result<()> {
        validate_curve(&self.width, 1, CurveDomain::Positive, "paragraph_width")?;
        validate_curve(&self.line_height, 1, CurveDomain::Positive, "line_height")?;
        validate_curve(
            &self.first_line_indent,
            1,
            CurveDomain::Finite,
            "first_line_indent",
        )?;
        validate_curve(&self.start_indent, 1, CurveDomain::Finite, "start_indent")?;
        validate_curve(&self.end_indent, 1, CurveDomain::Finite, "end_indent")?;
        validate_curve(
            &self.space_before,
            1,
            CurveDomain::Nonnegative,
            "space_before",
        )?;
        validate_curve(
            &self.space_after,
            1,
            CurveDomain::Nonnegative,
            "space_after",
        )?;
        validate_discrete_curve(&self.alignment, TextAlignment::from_code, "alignment")?;
        validate_discrete_curve(&self.direction, TextDirection::from_code, "direction")?;
        validate_discrete_curve(&self.wrap, TextWrap::from_code, "wrap")?;
        validate_common_curves(self.curves())
    }

    fn curves(&self) -> Vec<&AnimationCurve> {
        vec![
            &self.width,
            &self.line_height,
            &self.first_line_indent,
            &self.start_indent,
            &self.end_indent,
            &self.space_before,
            &self.space_after,
            &self.alignment,
            &self.direction,
            &self.wrap,
        ]
    }

    fn retimed(&self, start: RationalTime, end: RationalTime) -> Result<Self> {
        Self::new(
            self.width.retimed(start, end)?,
            self.line_height.retimed(start, end)?,
            self.first_line_indent.retimed(start, end)?,
            self.start_indent.retimed(start, end)?,
            self.end_indent.retimed(start, end)?,
            self.space_before.retimed(start, end)?,
            self.space_after.retimed(start, end)?,
            self.alignment.retimed(start, end)?,
            self.direction.retimed(start, end)?,
            self.wrap.retimed(start, end)?,
        )
    }
}

/// One paragraph style over one complete logical paragraph range.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct ParagraphSpan {
    range: TextRange,
    style: ParagraphStyle,
}

impl ParagraphSpan {
    /// Creates a nonempty paragraph span. Layer construction checks paragraph boundaries.
    pub fn new(range: TextRange, style: ParagraphStyle) -> Result<Self> {
        if range.is_empty() {
            return Err(empty_span("paragraph"));
        }
        Ok(Self { range, style })
    }

    /// Returns its complete logical paragraph range.
    #[must_use]
    pub const fn range(&self) -> TextRange {
        self.range
    }

    /// Returns complete animated paragraph controls.
    #[must_use]
    pub const fn style(&self) -> &ParagraphStyle {
        &self.style
    }
}

/// Complete versioned editable text, style, paragraph, and animation state.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLayer {
    text: String,
    style_spans: Vec<TextStyleSpan>,
    paragraph_spans: Vec<ParagraphSpan>,
    timebase: Timebase,
}

impl TextLayer {
    /// Creates checked canonical state with exact UTF-8 coverage and one animation clock.
    pub fn new(
        text: impl Into<String>,
        style_spans: impl IntoIterator<Item = TextStyleSpan>,
        paragraph_spans: impl IntoIterator<Item = ParagraphSpan>,
    ) -> Result<Self> {
        let text = text.into();
        let style_spans = collect_bounded(style_spans, MAX_TEXT_SPANS, "style_span_limit")?;
        let paragraph_spans =
            collect_bounded(paragraph_spans, MAX_TEXT_SPANS, "paragraph_span_limit")?;
        Self::from_parts(text, style_spans, paragraph_spans)
    }

    fn from_parts(
        text: String,
        style_spans: Vec<TextStyleSpan>,
        paragraph_spans: Vec<ParagraphSpan>,
    ) -> Result<Self> {
        if text.is_empty() {
            return Err(text_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "create_layer",
                "empty_text",
                "text layer must contain at least one UTF-8 scalar",
            ));
        }
        if text.len() > MAX_TEXT_BYTES {
            return Err(limit_error("text_byte_limit", text.len(), MAX_TEXT_BYTES));
        }
        validate_span_coverage(&text, style_spans.iter().map(|span| span.range), "style")?;
        validate_span_coverage(
            &text,
            paragraph_spans.iter().map(|span| span.range),
            "paragraph",
        )?;
        validate_paragraph_boundaries(&text, &paragraph_spans)?;
        for span in &style_spans {
            span.style.validate()?;
        }
        for span in &paragraph_spans {
            span.style.validate()?;
        }
        let all_curves = style_spans
            .iter()
            .flat_map(|span| span.style.curves())
            .chain(paragraph_spans.iter().flat_map(|span| span.style.curves()))
            .collect::<Vec<_>>();
        validate_common_curves(all_curves.clone())?;
        let timebase = all_curves.first().ok_or_else(clock_missing)?.timebase();
        Ok(Self {
            text,
            style_spans,
            paragraph_spans,
            timebase,
        })
    }

    /// Returns the complete UTF-8 content.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Returns canonical adjacent style coverage.
    #[must_use]
    pub fn style_spans(&self) -> &[TextStyleSpan] {
        &self.style_spans
    }

    /// Returns canonical complete paragraph coverage.
    #[must_use]
    pub fn paragraph_spans(&self) -> &[ParagraphSpan] {
        &self.paragraph_spans
    }

    /// Returns the exact animation clock shared by every visual control.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.timebase
    }

    /// Replaces UTF-8 inside one current style and paragraph while preserving all visual state.
    ///
    /// Newline topology is deliberately explicit. This focused edit rejects a different newline
    /// count, after which callers may reconstruct paragraph spans through `TextLayer::new`.
    pub fn with_replaced_text(&self, range: TextRange, replacement: &str) -> Result<Self> {
        validate_text_range(&self.text, range, true, "replace_text")?;
        if self.text[range.start..range.end].matches('\n').count()
            != replacement.matches('\n').count()
        {
            return Err(text_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "replace_text",
                "paragraph_topology_changed",
                "text replacement must preserve newline count or provide new paragraph spans",
            ));
        }
        if !self
            .style_spans
            .iter()
            .any(|span| span.range.contains_range(range))
            || !self
                .paragraph_spans
                .iter()
                .any(|span| span.range.contains_range(range))
        {
            return Err(text_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "replace_text",
                "replacement_crosses_span",
                "focused text replacement must stay inside one style and paragraph span",
            ));
        }
        let new_len = self
            .text
            .len()
            .checked_sub(range.len())
            .and_then(|length| length.checked_add(replacement.len()))
            .ok_or_else(|| limit_error("text_byte_limit", usize::MAX, MAX_TEXT_BYTES))?;
        if new_len > MAX_TEXT_BYTES {
            return Err(limit_error("text_byte_limit", new_len, MAX_TEXT_BYTES));
        }
        let mut text = self.text.clone();
        text.replace_range(range.start..range.end, replacement);
        let delta = isize::try_from(replacement.len())
            .ok()
            .and_then(|new| isize::try_from(range.len()).ok().map(|old| new - old))
            .ok_or_else(|| limit_error("text_byte_limit", new_len, MAX_TEXT_BYTES))?;
        let style_spans = self
            .style_spans
            .iter()
            .map(|span| {
                Ok(TextStyleSpan {
                    range: shifted_range(span.range, range, delta)?,
                    style: span.style.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        let paragraph_spans = self
            .paragraph_spans
            .iter()
            .map(|span| {
                Ok(ParagraphSpan {
                    range: shifted_range(span.range, range, delta)?,
                    style: span.style.clone(),
                })
            })
            .collect::<Result<Vec<_>>>()?;
        Self::from_parts(text, style_spans, paragraph_spans)
    }

    /// Applies one complete style over an exact nonempty UTF-8 range.
    pub fn with_style(&self, range: TextRange, style: TextStyle) -> Result<Self> {
        validate_text_range(&self.text, range, false, "apply_style")?;
        style.validate()?;
        let mut spans = Vec::with_capacity(self.style_spans.len().saturating_add(2));
        let mut inserted = false;
        for span in &self.style_spans {
            if span.range.end <= range.start || range.end <= span.range.start {
                spans.push(span.clone());
                continue;
            }
            if span.range.start < range.start {
                spans.push(TextStyleSpan::new(
                    TextRange::new(span.range.start, range.start)?,
                    span.style.clone(),
                )?);
            }
            if !inserted {
                spans.push(TextStyleSpan::new(range, style.clone())?);
                inserted = true;
            }
            if range.end < span.range.end {
                spans.push(TextStyleSpan::new(
                    TextRange::new(range.end, span.range.end)?,
                    span.style.clone(),
                )?);
            }
        }
        let mut merged: Vec<TextStyleSpan> = Vec::with_capacity(spans.len());
        for span in spans {
            if let Some(previous) = merged.last_mut() {
                if previous.range.end == span.range.start && previous.style == span.style {
                    previous.range.end = span.range.end;
                    continue;
                }
            }
            merged.push(span);
        }
        Self::from_parts(self.text.clone(), merged, self.paragraph_spans.clone())
    }

    /// Replaces the controls for one exact existing paragraph range.
    pub fn with_paragraph_style(&self, range: TextRange, style: ParagraphStyle) -> Result<Self> {
        validate_text_range(&self.text, range, false, "apply_paragraph_style")?;
        style.validate()?;
        let index = self
            .paragraph_spans
            .iter()
            .position(|span| span.range == range)
            .ok_or_else(|| {
                text_error(
                    ErrorCategory::NotFound,
                    Recoverability::UserCorrectable,
                    "apply_paragraph_style",
                    "unknown_paragraph_range",
                    "paragraph style range must match one complete existing paragraph",
                )
            })?;
        let mut paragraphs = self.paragraph_spans.clone();
        paragraphs[index] = ParagraphSpan::new(range, style)?;
        Self::from_parts(self.text.clone(), self.style_spans.clone(), paragraphs)
    }

    /// Uniformly retimes every continuous and discrete visual control.
    pub fn retimed(&self, new_start: RationalTime, new_end: RationalTime) -> Result<Self> {
        let style_spans = self
            .style_spans
            .iter()
            .map(|span| TextStyleSpan::new(span.range, span.style.retimed(new_start, new_end)?))
            .collect::<Result<Vec<_>>>()?;
        let paragraph_spans = self
            .paragraph_spans
            .iter()
            .map(|span| ParagraphSpan::new(span.range, span.style.retimed(new_start, new_end)?))
            .collect::<Result<Vec<_>>>()?;
        Self::from_parts(self.text.clone(), style_spans, paragraph_spans)
    }
}

impl Serialize for TextLayer {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        TextLayerWireRef {
            schema_revision: TEXT_LAYER_SCHEMA_REVISION,
            text: &self.text,
            style_spans: &self.style_spans,
            paragraph_spans: &self.paragraph_spans,
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for TextLayer {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        TextLayerWire::deserialize(deserializer)?
            .into_layer()
            .map_err(D::Error::custom)
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct TextLayerWireRef<'a> {
    schema_revision: u32,
    text: &'a str,
    style_spans: &'a [TextStyleSpan],
    paragraph_spans: &'a [ParagraphSpan],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TextLayerWire {
    schema_revision: u32,
    #[serde(deserialize_with = "deserialize_bounded_text")]
    text: String,
    #[serde(deserialize_with = "deserialize_bounded_style_spans")]
    style_spans: Vec<TextStyleSpanWire>,
    #[serde(deserialize_with = "deserialize_bounded_paragraph_spans")]
    paragraph_spans: Vec<ParagraphSpanWire>,
}

impl TextLayerWire {
    fn into_layer(self) -> Result<TextLayer> {
        if self.schema_revision != TEXT_LAYER_SCHEMA_REVISION {
            return Err(text_error(
                ErrorCategory::Unsupported,
                Recoverability::UserCorrectable,
                "deserialize_layer",
                "unsupported_schema_revision",
                "text layer schema revision is not supported",
            ));
        }
        TextLayer::new(
            self.text,
            self.style_spans
                .into_iter()
                .map(TextStyleSpanWire::into_span)
                .collect::<Result<Vec<_>>>()?,
            self.paragraph_spans
                .into_iter()
                .map(ParagraphSpanWire::into_span)
                .collect::<Result<Vec<_>>>()?,
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TextRangeWire {
    start: usize,
    end: usize,
}

impl TextRangeWire {
    fn into_range(self) -> Result<TextRange> {
        TextRange::new(self.start, self.end)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FontFaceWire {
    asset_id: String,
    family: String,
    display_name: String,
    collection_index: u32,
}

impl FontFaceWire {
    fn into_face(self) -> Result<FontFace> {
        FontFace::new(
            self.asset_id,
            self.family,
            self.display_name,
            self.collection_index,
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct OpenTypeFeatureWire {
    tag: [u8; 4],
    value: u16,
}

impl OpenTypeFeatureWire {
    fn into_feature(self) -> Result<OpenTypeFeature> {
        OpenTypeFeature::new(self.tag, self.value)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct VariationAxisWire {
    tag: [u8; 4],
    value: AnimationCurve,
}

impl VariationAxisWire {
    fn into_axis(self) -> Result<VariationAxis> {
        VariationAxis::new(self.tag, self.value)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TextStyleWire {
    font: FontFaceWire,
    font_size: AnimationCurve,
    fill_rgba: AnimationCurve,
    opacity: AnimationCurve,
    tracking: AnimationCurve,
    baseline_shift: AnimationCurve,
    #[serde(deserialize_with = "deserialize_bounded_features")]
    features: Vec<OpenTypeFeatureWire>,
    #[serde(deserialize_with = "deserialize_bounded_variations")]
    variations: Vec<VariationAxisWire>,
}

impl TextStyleWire {
    fn into_style(self) -> Result<TextStyle> {
        TextStyle::new(
            self.font.into_face()?,
            self.font_size,
            self.fill_rgba,
            self.opacity,
            self.tracking,
            self.baseline_shift,
            self.features
                .into_iter()
                .map(OpenTypeFeatureWire::into_feature)
                .collect::<Result<Vec<_>>>()?,
            self.variations
                .into_iter()
                .map(VariationAxisWire::into_axis)
                .collect::<Result<Vec<_>>>()?,
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TextStyleSpanWire {
    range: TextRangeWire,
    style: TextStyleWire,
}

impl TextStyleSpanWire {
    fn into_span(self) -> Result<TextStyleSpan> {
        TextStyleSpan::new(self.range.into_range()?, self.style.into_style()?)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParagraphStyleWire {
    width: AnimationCurve,
    line_height: AnimationCurve,
    first_line_indent: AnimationCurve,
    start_indent: AnimationCurve,
    end_indent: AnimationCurve,
    space_before: AnimationCurve,
    space_after: AnimationCurve,
    alignment: AnimationCurve,
    direction: AnimationCurve,
    wrap: AnimationCurve,
}

impl ParagraphStyleWire {
    fn into_style(self) -> Result<ParagraphStyle> {
        ParagraphStyle::new(
            self.width,
            self.line_height,
            self.first_line_indent,
            self.start_indent,
            self.end_indent,
            self.space_before,
            self.space_after,
            self.alignment,
            self.direction,
            self.wrap,
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ParagraphSpanWire {
    range: TextRangeWire,
    style: ParagraphStyleWire,
}

impl ParagraphSpanWire {
    fn into_span(self) -> Result<ParagraphSpan> {
        ParagraphSpan::new(self.range.into_range()?, self.style.into_style()?)
    }
}

fn deserialize_bounded_text<'de, D>(deserializer: D) -> std::result::Result<String, D::Error>
where
    D: Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    if value.len() > MAX_TEXT_BYTES {
        return Err(D::Error::custom("text exceeds the supported byte bound"));
    }
    Ok(value)
}

fn deserialize_bounded_style_spans<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<TextStyleSpanWire>, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_seq(BoundedVecVisitor::new(MAX_TEXT_SPANS, "text style spans"))
}

fn deserialize_bounded_paragraph_spans<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<ParagraphSpanWire>, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_seq(BoundedVecVisitor::new(MAX_TEXT_SPANS, "paragraph spans"))
}

fn deserialize_bounded_features<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<OpenTypeFeatureWire>, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_seq(BoundedVecVisitor::new(MAX_FONT_SETTINGS, "font features"))
}

fn deserialize_bounded_variations<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<VariationAxisWire>, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_seq(BoundedVecVisitor::new(MAX_FONT_SETTINGS, "font variations"))
}

struct BoundedVecVisitor<T> {
    limit: usize,
    description: &'static str,
    marker: std::marker::PhantomData<fn() -> T>,
}

impl<T> BoundedVecVisitor<T> {
    const fn new(limit: usize, description: &'static str) -> Self {
        Self {
            limit,
            description,
            marker: std::marker::PhantomData,
        }
    }
}

impl<'de, T> Visitor<'de> for BoundedVecVisitor<T>
where
    T: Deserialize<'de>,
{
    type Value = Vec<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "at most {} {}", self.limit, self.description)
    }

    fn visit_seq<A>(self, mut sequence: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        if sequence.size_hint().is_some_and(|hint| hint > self.limit) {
            return Err(A::Error::custom("collection exceeds its supported bound"));
        }
        let mut values = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(self.limit));
        loop {
            if values.len() == self.limit {
                if sequence.next_element::<IgnoredAny>()?.is_some() {
                    return Err(A::Error::custom("collection exceeds its supported bound"));
                }
                return Ok(values);
            }
            let Some(value) = sequence.next_element()? else {
                return Ok(values);
            };
            values.push(value);
        }
    }
}

/// Caller-owned offline mapping from persistent font references to exact bytes.
pub trait FontResolver {
    /// Resolves one font asset without system enumeration or network access.
    fn resolve(&self, font: &FontFace) -> Result<Arc<[u8]>>;
}

/// One positioned glyph with its exact logical source cluster.
#[derive(Clone, Debug, PartialEq)]
pub struct PositionedGlyph {
    glyph_id: u16,
    source_range: TextRange,
    x: f64,
    y: f64,
    advance: f64,
}

impl PositionedGlyph {
    /// Returns the font-local glyph identifier.
    #[must_use]
    pub const fn glyph_id(&self) -> u16 {
        self.glyph_id
    }

    /// Returns the complete logical UTF-8 cluster range.
    #[must_use]
    pub const fn source_range(&self) -> TextRange {
        self.source_range
    }

    /// Returns the positioned horizontal origin in layout pixels.
    #[must_use]
    pub const fn x(&self) -> f64 {
        self.x
    }

    /// Returns the positioned vertical origin in layout pixels.
    #[must_use]
    pub const fn y(&self) -> f64 {
        self.y
    }

    /// Returns the shaped horizontal advance.
    #[must_use]
    pub const fn advance(&self) -> f64 {
        self.advance
    }
}

/// One visually ordered run sharing exact typography and bidi direction.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLayoutRun {
    source_range: TextRange,
    style_span_index: usize,
    direction: TextDirection,
    font: FontFace,
    font_size: f64,
    fill_rgba: [f64; 4],
    opacity: f64,
    ascent: f64,
    descent: f64,
    glyphs: Vec<PositionedGlyph>,
}

impl TextLayoutRun {
    /// Returns the bounding logical source range of every visual cluster in the run.
    #[must_use]
    pub const fn source_range(&self) -> TextRange {
        self.source_range
    }

    /// Returns the authored style span index.
    #[must_use]
    pub const fn style_span_index(&self) -> usize {
        self.style_span_index
    }

    /// Returns the resolved visual run direction.
    #[must_use]
    pub const fn direction(&self) -> TextDirection {
        self.direction
    }

    /// Returns the persistent font reference.
    #[must_use]
    pub const fn font(&self) -> &FontFace {
        &self.font
    }

    /// Returns the sampled font size.
    #[must_use]
    pub const fn font_size(&self) -> f64 {
        self.font_size
    }

    /// Returns the sampled RGBA fill.
    #[must_use]
    pub const fn fill_rgba(&self) -> [f64; 4] {
        self.fill_rgba
    }

    /// Returns the sampled normalized opacity.
    #[must_use]
    pub const fn opacity(&self) -> f64 {
        self.opacity
    }

    /// Returns the font ascent at the sampled size.
    #[must_use]
    pub const fn ascent(&self) -> f64 {
        self.ascent
    }

    /// Returns the font descent at the sampled size.
    #[must_use]
    pub const fn descent(&self) -> f64 {
        self.descent
    }

    /// Returns positioned glyphs in visual cluster order.
    #[must_use]
    pub fn glyphs(&self) -> &[PositionedGlyph] {
        &self.glyphs
    }
}

/// One positioned visual line.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLayoutLine {
    source_range: TextRange,
    paragraph_index: usize,
    line_index_in_paragraph: usize,
    origin_x: f64,
    baseline_y: f64,
    width: f64,
    height: f64,
    runs: Vec<TextLayoutRun>,
}

impl TextLayoutLine {
    /// Returns the complete logical source coverage.
    #[must_use]
    pub const fn source_range(&self) -> TextRange {
        self.source_range
    }

    /// Returns the authored paragraph index.
    #[must_use]
    pub const fn paragraph_index(&self) -> usize {
        self.paragraph_index
    }

    /// Returns the line index inside its paragraph.
    #[must_use]
    pub const fn line_index_in_paragraph(&self) -> usize {
        self.line_index_in_paragraph
    }

    /// Returns the aligned horizontal origin.
    #[must_use]
    pub const fn origin_x(&self) -> f64 {
        self.origin_x
    }

    /// Returns the baseline position.
    #[must_use]
    pub const fn baseline_y(&self) -> f64 {
        self.baseline_y
    }

    /// Returns the positioned line width.
    #[must_use]
    pub const fn width(&self) -> f64 {
        self.width
    }

    /// Returns the authored line height.
    #[must_use]
    pub const fn height(&self) -> f64 {
        self.height
    }

    /// Returns visually ordered runs.
    #[must_use]
    pub fn runs(&self) -> &[TextLayoutRun] {
        &self.runs
    }
}

/// Complete owned CPU layout for a later raster and GPU owner.
#[derive(Clone, Debug, PartialEq)]
pub struct TextLayout {
    time: RationalTime,
    width: f64,
    height: f64,
    lines: Vec<TextLayoutLine>,
}

impl TextLayout {
    /// Returns the exact sample time requested by the caller.
    #[must_use]
    pub const fn time(&self) -> RationalTime {
        self.time
    }

    /// Returns the largest authored paragraph width.
    #[must_use]
    pub const fn width(&self) -> f64 {
        self.width
    }

    /// Returns the complete vertical layout extent.
    #[must_use]
    pub const fn height(&self) -> f64 {
        self.height
    }

    /// Returns positioned lines in logical paragraph order.
    #[must_use]
    pub fn lines(&self) -> &[TextLayoutLine] {
        &self.lines
    }
}

/// Stateful shaping cache with no font discovery or hidden authored state.
pub struct TextLayoutEngine {
    shape_context: ShapeContext,
}

impl TextLayoutEngine {
    /// Creates an empty bounded shaping context.
    #[must_use]
    pub fn new() -> Self {
        Self {
            shape_context: ShapeContext::with_max_entries(32),
        }
    }

    /// Samples, itemizes, shapes, wraps, reorders, and positions one immutable layer.
    pub fn layout<R: FontResolver + ?Sized>(
        &mut self,
        layer: &TextLayer,
        time: RationalTime,
        resolver: &R,
    ) -> Result<TextLayout> {
        layout_text(&mut self.shape_context, layer, time, resolver)
    }
}

impl Default for TextLayoutEngine {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone)]
struct SampledTextStyle {
    font: FontFace,
    font_size: f64,
    fill_rgba: [f64; 4],
    opacity: f64,
    tracking: f64,
    baseline_shift: f64,
    features: Vec<(u32, u16)>,
    variations: Vec<(u32, f32)>,
}

#[derive(Clone, Copy)]
struct SampledParagraphStyle {
    width: f64,
    line_height: f64,
    first_line_indent: f64,
    start_indent: f64,
    end_indent: f64,
    space_before: f64,
    space_after: f64,
    alignment: TextAlignment,
    direction: TextDirection,
    wrap: TextWrap,
}

#[derive(Clone)]
struct RawGlyph {
    id: u16,
    x: f64,
    y: f64,
    advance: f64,
}

#[derive(Clone)]
struct ShapedCluster {
    source_range: TextRange,
    style_span_index: usize,
    level: Level,
    style: SampledTextStyle,
    ascent: f64,
    descent: f64,
    advance: f64,
    glyphs: Vec<RawGlyph>,
}

#[derive(Clone, Copy)]
struct ShapeItem {
    start: usize,
    end: usize,
    style_span_index: usize,
    level: Level,
    script: Script,
}

fn layout_text<R: FontResolver + ?Sized>(
    shape_context: &mut ShapeContext,
    layer: &TextLayer,
    time: RationalTime,
    resolver: &R,
) -> Result<TextLayout> {
    let time = time.checked_rescale(layer.timebase, TimeRounding::Exact)?;
    let sampled_styles = layer
        .style_spans
        .iter()
        .map(|span| sample_text_style(&span.style, time))
        .collect::<Result<Vec<_>>>()?;
    let mut lines = Vec::new();
    let mut y = 0.0;
    let mut layout_width: f64 = 0.0;
    let mut glyph_count = 0usize;

    for (paragraph_index, paragraph) in layer.paragraph_spans.iter().enumerate() {
        let paragraph_style = sample_paragraph_style(&paragraph.style, time)?;
        layout_width = layout_width.max(paragraph_style.width);
        y += paragraph_style.space_before;
        let content_end = if layer.text.as_bytes()[paragraph.range.end - 1] == b'\n' {
            paragraph.range.end - 1
        } else {
            paragraph.range.end
        };
        let content_range = TextRange::new(paragraph.range.start, content_end)?;
        let paragraph_text = &layer.text[content_range.start..content_range.end];
        let base_level = match paragraph_style.direction {
            TextDirection::Auto => None,
            TextDirection::LeftToRight => Some(Level::ltr()),
            TextDirection::RightToLeft => Some(Level::rtl()),
        };
        let bidi = BidiInfo::new(paragraph_text, base_level);
        let resolved_base = bidi
            .paragraphs
            .first()
            .map_or(base_level.unwrap_or_else(Level::ltr), |info| info.level);
        let items = shape_items(layer, content_range, &bidi, resolved_base)?;
        let mut clusters = Vec::new();
        for item in items {
            let item_clusters = shape_item(
                shape_context,
                layer,
                item,
                &sampled_styles[item.style_span_index],
                resolver,
            )?;
            glyph_count = glyph_count
                .checked_add(
                    item_clusters
                        .iter()
                        .map(|cluster| cluster.glyphs.len())
                        .sum::<usize>(),
                )
                .ok_or_else(|| limit_error("glyph_limit", usize::MAX, MAX_LAYOUT_GLYPHS))?;
            if glyph_count > MAX_LAYOUT_GLYPHS {
                return Err(limit_error("glyph_limit", glyph_count, MAX_LAYOUT_GLYPHS));
            }
            clusters.extend(item_clusters);
        }
        let breaks = collect_line_breaks(paragraph_text, content_range.start);
        let line_slices = line_slices(&clusters, &breaks, paragraph_style)?;
        let line_count = line_slices.len();
        for (line_index, (start, end)) in line_slices.into_iter().enumerate() {
            let first_line = line_index == 0;
            let last_line = line_index + 1 == line_count;
            let indent = paragraph_style.start_indent
                + if first_line {
                    paragraph_style.first_line_indent
                } else {
                    0.0
                };
            let available = checked_measure(paragraph_style, first_line)?;
            let mut visual = clusters[start..end].to_vec();
            reorder_visual(&mut visual);
            let natural_width = visual.iter().map(|cluster| cluster.advance).sum::<f64>();
            let base_direction = if resolved_base.is_rtl() {
                TextDirection::RightToLeft
            } else {
                TextDirection::LeftToRight
            };
            let justify = paragraph_style.alignment == TextAlignment::Justify && !last_line;
            let spaces = if justify {
                visual
                    .iter()
                    .filter(|cluster| {
                        layer.text[cluster.source_range.start..cluster.source_range.end]
                            .chars()
                            .all(char::is_whitespace)
                    })
                    .count()
            } else {
                0
            };
            let extra_space = if spaces > 0 && natural_width < available {
                (available - natural_width) / spaces as f64
            } else {
                0.0
            };
            let positioned_width = natural_width + extra_space * spaces as f64;
            let alignment_offset = alignment_offset(
                paragraph_style.alignment,
                base_direction,
                available,
                positioned_width,
            );
            let origin_x = indent + alignment_offset;
            let max_ascent = visual
                .iter()
                .map(|cluster| cluster.ascent)
                .fold(0.0, f64::max);
            let baseline_y = y + max_ascent;
            let runs = position_runs(&layer.text, &visual, origin_x, baseline_y, extra_space)?;
            let source_range = if let (Some(first), Some(last)) =
                (clusters.get(start), clusters.get(end.saturating_sub(1)))
            {
                TextRange::new(first.source_range.start, last.source_range.end)?
            } else {
                TextRange::new(content_range.start, content_range.start)?
            };
            lines.push(TextLayoutLine {
                source_range,
                paragraph_index,
                line_index_in_paragraph: line_index,
                origin_x,
                baseline_y,
                width: positioned_width,
                height: paragraph_style.line_height,
                runs,
            });
            y += paragraph_style.line_height;
        }
        y += paragraph_style.space_after;
    }

    ensure_finite(layout_width, "layout_width")?;
    ensure_finite(y, "layout_height")?;
    Ok(TextLayout {
        time,
        width: layout_width,
        height: y,
        lines,
    })
}

fn shape_items(
    layer: &TextLayer,
    range: TextRange,
    bidi: &BidiInfo<'_>,
    base_level: Level,
) -> Result<Vec<ShapeItem>> {
    if range.is_empty() {
        return Ok(Vec::new());
    }
    let text = &layer.text[range.start..range.end];
    let mut items: Vec<ShapeItem> = Vec::new();
    let mut inherited_script = Script::Latin;
    for (local_start, character) in text.char_indices() {
        let local_end = local_start + character.len_utf8();
        let global_start = range.start + local_start;
        let style_span_index = layer
            .style_spans
            .iter()
            .position(|span| span.range.start <= global_start && global_start < span.range.end)
            .ok_or_else(|| corrupt_layout("style_lookup_failed"))?;
        let level = bidi.levels.get(local_start).copied().unwrap_or(base_level);
        let raw_script = character.script();
        let script = if matches!(raw_script, Script::Common | Script::Inherited) {
            inherited_script
        } else {
            inherited_script = raw_script;
            raw_script
        };
        if let Some(item) = items.last_mut() {
            if item.end == global_start
                && item.style_span_index == style_span_index
                && item.level == level
                && item.script == script
            {
                item.end = range.start + local_end;
                continue;
            }
        }
        items.push(ShapeItem {
            start: global_start,
            end: range.start + local_end,
            style_span_index,
            level,
            script,
        });
    }
    Ok(items)
}

fn shape_item<R: FontResolver + ?Sized>(
    context: &mut ShapeContext,
    layer: &TextLayer,
    item: ShapeItem,
    style: &SampledTextStyle,
    resolver: &R,
) -> Result<Vec<ShapedCluster>> {
    let bytes = resolver.resolve(&style.font)?;
    let face_index = style.font.collection_index;
    skrifa::FontRef::from_index(bytes.as_ref(), face_index)
        .map_err(|_| invalid_font(&style.font))?;
    let swash_index = usize::try_from(face_index).map_err(|_| invalid_font(&style.font))?;
    let font = FontRef::from_index(bytes.as_ref(), swash_index)
        .ok_or_else(|| invalid_font(&style.font))?;
    let direction = if item.level.is_rtl() {
        ShapeDirection::RightToLeft
    } else {
        ShapeDirection::LeftToRight
    };
    let mut shaper = context
        .builder(font)
        .script(item.script)
        .direction(direction)
        .size(style.font_size as f32)
        .features(style.features.iter().copied())
        .variations(style.variations.iter().copied())
        .build();
    let metrics = shaper.metrics();
    shaper.add_str(&layer.text[item.start..item.end]);
    let mut clusters = Vec::new();
    shaper.shape_with(|cluster| {
        let source_start = item.start + cluster.source.start as usize;
        let source_end = item.start + cluster.source.end as usize;
        let glyphs = cluster
            .glyphs
            .iter()
            .map(|glyph| RawGlyph {
                id: glyph.id,
                x: f64::from(glyph.x),
                y: f64::from(glyph.y),
                advance: f64::from(glyph.advance),
            })
            .collect::<Vec<_>>();
        clusters.push(ShapedCluster {
            source_range: TextRange {
                start: source_start,
                end: source_end,
            },
            style_span_index: item.style_span_index,
            level: item.level,
            style: style.clone(),
            ascent: f64::from(metrics.ascent),
            descent: f64::from(metrics.descent),
            advance: f64::from(cluster.advance()) + style.tracking,
            glyphs,
        });
    });
    for cluster in &clusters {
        validate_text_range(&layer.text, cluster.source_range, false, "shape_text")?;
        ensure_finite(cluster.advance, "cluster_advance")?;
        ensure_finite(cluster.ascent, "font_ascent")?;
        ensure_finite(cluster.descent, "font_descent")?;
        for glyph in &cluster.glyphs {
            for (field, value) in [
                ("glyph_x", glyph.x),
                ("glyph_y", glyph.y),
                ("glyph_advance", glyph.advance),
            ] {
                ensure_finite(value, field)?;
            }
        }
    }
    Ok(clusters)
}

fn collect_line_breaks(text: &str, global_start: usize) -> BTreeMap<usize, BreakOpportunity> {
    linebreaks(text)
        .map(|(position, opportunity)| (global_start + position, opportunity))
        .collect()
}

fn line_slices(
    clusters: &[ShapedCluster],
    breaks: &BTreeMap<usize, BreakOpportunity>,
    style: SampledParagraphStyle,
) -> Result<Vec<(usize, usize)>> {
    if clusters.is_empty() {
        return Ok(vec![(0, 0)]);
    }
    if style.wrap == TextWrap::NoWrap {
        return Ok(vec![(0, clusters.len())]);
    }
    let mut result = Vec::new();
    let mut start = 0;
    while start < clusters.len() {
        let available = checked_measure(style, result.is_empty())?;
        let mut end = start;
        let mut width = 0.0;
        let mut allowed = None;
        let mut selected = None;
        while end < clusters.len() {
            let next_width = width + clusters[end].advance;
            if next_width > available && end > start {
                if let Some(candidate) = allowed.filter(|candidate| *candidate > start) {
                    selected = Some(candidate);
                    break;
                }
                if style.wrap == TextWrap::Anywhere {
                    selected = Some(end);
                    break;
                }
            }
            width = next_width;
            end += 1;
            if breaks.contains_key(&clusters[end - 1].source_range.end) {
                allowed = Some(end);
                if width > available && style.wrap == TextWrap::Word {
                    selected = Some(end);
                    break;
                }
            }
        }
        let selected = selected.unwrap_or(end).max(start + 1);
        result.push((start, selected));
        start = selected;
    }
    Ok(result)
}

fn reorder_visual(clusters: &mut [ShapedCluster]) {
    let Some(max_level) = clusters.iter().map(|cluster| cluster.level.number()).max() else {
        return;
    };
    let Some(min_odd) = clusters
        .iter()
        .map(|cluster| cluster.level.number())
        .filter(|level| level % 2 == 1)
        .min()
    else {
        return;
    };
    for level in (min_odd..=max_level).rev() {
        let mut start = 0;
        while start < clusters.len() {
            while start < clusters.len() && clusters[start].level.number() < level {
                start += 1;
            }
            let mut end = start;
            while end < clusters.len() && clusters[end].level.number() >= level {
                end += 1;
            }
            clusters[start..end].reverse();
            start = end;
        }
    }
}

fn position_runs(
    text: &str,
    clusters: &[ShapedCluster],
    origin_x: f64,
    baseline_y: f64,
    extra_space: f64,
) -> Result<Vec<TextLayoutRun>> {
    let mut runs: Vec<TextLayoutRun> = Vec::new();
    let mut cursor = origin_x;
    for cluster in clusters {
        let direction = if cluster.level.is_rtl() {
            TextDirection::RightToLeft
        } else {
            TextDirection::LeftToRight
        };
        let needs_new_run = match runs.last() {
            Some(run) => {
                run.style_span_index != cluster.style_span_index || run.direction != direction
            }
            None => true,
        };
        if needs_new_run {
            runs.push(TextLayoutRun {
                source_range: cluster.source_range,
                style_span_index: cluster.style_span_index,
                direction,
                font: cluster.style.font.clone(),
                font_size: cluster.style.font_size,
                fill_rgba: cluster.style.fill_rgba,
                opacity: cluster.style.opacity,
                ascent: cluster.ascent,
                descent: cluster.descent,
                glyphs: Vec::new(),
            });
        }
        let run = runs.last_mut().expect("run was created");
        run.source_range.start = run.source_range.start.min(cluster.source_range.start);
        run.source_range.end = run.source_range.end.max(cluster.source_range.end);
        let mut glyph_cursor = cursor;
        for glyph in &cluster.glyphs {
            let positioned = PositionedGlyph {
                glyph_id: glyph.id,
                source_range: cluster.source_range,
                x: glyph_cursor + glyph.x,
                y: baseline_y - glyph.y - cluster.style.baseline_shift,
                advance: glyph.advance,
            };
            ensure_finite(positioned.x, "positioned_glyph_x")?;
            ensure_finite(positioned.y, "positioned_glyph_y")?;
            run.glyphs.push(positioned);
            glyph_cursor += glyph.advance;
        }
        cursor += cluster.advance;
        if text[cluster.source_range.start..cluster.source_range.end]
            .chars()
            .all(char::is_whitespace)
        {
            cursor += extra_space;
        }
    }
    Ok(runs)
}

fn sample_text_style(style: &TextStyle, time: RationalTime) -> Result<SampledTextStyle> {
    let font_size = sample_scalar(&style.font_size, time, "font_size")?;
    validate_sample(font_size, CurveDomain::Positive, "font_size")?;
    let fill = sample_components(&style.fill_rgba, time, 4, "fill_rgba")?;
    for value in &fill {
        validate_sample(*value, CurveDomain::Normalized, "fill_rgba")?;
    }
    let opacity = sample_scalar(&style.opacity, time, "opacity")?;
    validate_sample(opacity, CurveDomain::Normalized, "opacity")?;
    let tracking = sample_scalar(&style.tracking, time, "tracking")?;
    validate_sample(tracking, CurveDomain::Finite, "tracking")?;
    let baseline_shift = sample_scalar(&style.baseline_shift, time, "baseline_shift")?;
    validate_sample(baseline_shift, CurveDomain::Finite, "baseline_shift")?;
    let mut variations = Vec::with_capacity(style.variations.len());
    for axis in &style.variations {
        let value = sample_scalar(&axis.value, time, "variation_axis")?;
        validate_sample(value, CurveDomain::Finite, "variation_axis")?;
        variations.push((u32::from_be_bytes(axis.tag), value as f32));
    }
    Ok(SampledTextStyle {
        font: style.font.clone(),
        font_size,
        fill_rgba: [fill[0], fill[1], fill[2], fill[3]],
        opacity,
        tracking,
        baseline_shift,
        features: style
            .features
            .iter()
            .map(|feature| (u32::from_be_bytes(feature.tag), feature.value))
            .collect(),
        variations,
    })
}

fn sample_paragraph_style(
    style: &ParagraphStyle,
    time: RationalTime,
) -> Result<SampledParagraphStyle> {
    let sampled = SampledParagraphStyle {
        width: sample_scalar(&style.width, time, "paragraph_width")?,
        line_height: sample_scalar(&style.line_height, time, "line_height")?,
        first_line_indent: sample_scalar(&style.first_line_indent, time, "first_line_indent")?,
        start_indent: sample_scalar(&style.start_indent, time, "start_indent")?,
        end_indent: sample_scalar(&style.end_indent, time, "end_indent")?,
        space_before: sample_scalar(&style.space_before, time, "space_before")?,
        space_after: sample_scalar(&style.space_after, time, "space_after")?,
        alignment: sample_discrete(
            &style.alignment,
            time,
            TextAlignment::from_code,
            "alignment",
        )?,
        direction: sample_discrete(
            &style.direction,
            time,
            TextDirection::from_code,
            "direction",
        )?,
        wrap: sample_discrete(&style.wrap, time, TextWrap::from_code, "wrap")?,
    };
    validate_sample(sampled.width, CurveDomain::Positive, "paragraph_width")?;
    validate_sample(sampled.line_height, CurveDomain::Positive, "line_height")?;
    for (field, value) in [
        ("first_line_indent", sampled.first_line_indent),
        ("start_indent", sampled.start_indent),
        ("end_indent", sampled.end_indent),
    ] {
        validate_sample(value, CurveDomain::Finite, field)?;
    }
    validate_sample(
        sampled.space_before,
        CurveDomain::Nonnegative,
        "space_before",
    )?;
    validate_sample(sampled.space_after, CurveDomain::Nonnegative, "space_after")?;
    checked_measure(sampled, true)?;
    checked_measure(sampled, false)?;
    Ok(sampled)
}

fn checked_measure(style: SampledParagraphStyle, first_line: bool) -> Result<f64> {
    let indent = style.start_indent
        + style.end_indent
        + if first_line {
            style.first_line_indent
        } else {
            0.0
        };
    let measure = style.width - indent;
    if !measure.is_finite() || measure <= 0.0 {
        return Err(text_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "sample_paragraph",
            "nonpositive_line_measure",
            "sampled paragraph indents must leave a positive line measure",
        ));
    }
    Ok(measure)
}

fn alignment_offset(
    alignment: TextAlignment,
    base_direction: TextDirection,
    available: f64,
    width: f64,
) -> f64 {
    let remaining = (available - width).max(0.0);
    match alignment {
        TextAlignment::Center => remaining * 0.5,
        TextAlignment::Start if base_direction == TextDirection::RightToLeft => remaining,
        TextAlignment::End if base_direction == TextDirection::LeftToRight => remaining,
        TextAlignment::Start | TextAlignment::End | TextAlignment::Justify => 0.0,
    }
}

#[derive(Clone, Copy)]
enum CurveDomain {
    Finite,
    Positive,
    Nonnegative,
    Normalized,
}

fn validate_curve(
    curve: &AnimationCurve,
    components: usize,
    domain: CurveDomain,
    field: &'static str,
) -> Result<()> {
    for keyframe in curve.keyframes() {
        if keyframe.value().component_count() != components {
            return Err(text_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "validate_curve",
                "component_count_mismatch",
                "text animation curve has the wrong component count",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "validate_curve")
                    .with_field("field", field)
                    .with_field("expected_components", components.to_string()),
            ));
        }
        for value in keyframe.value().components() {
            validate_sample(*value, domain, field)?;
        }
    }
    Ok(())
}

fn validate_discrete_curve<T>(
    curve: &AnimationCurve,
    decode: impl Fn(f64) -> Option<T>,
    field: &'static str,
) -> Result<()> {
    validate_curve(curve, 1, CurveDomain::Finite, field)?;
    if curve.expression().is_some() {
        return Err(discrete_error(field));
    }
    for (index, keyframe) in curve.keyframes().iter().enumerate() {
        if decode(keyframe.value().components()[0]).is_none() {
            return Err(discrete_error(field));
        }
        if let Some(next) = curve.keyframes().get(index + 1) {
            let changes = keyframe.value() != next.value();
            if changes && keyframe.outgoing_interpolation() != Interpolation::Hold {
                return Err(discrete_error(field));
            }
        }
    }
    Ok(())
}

fn validate_common_curves(curves: Vec<&AnimationCurve>) -> Result<()> {
    let Some(first) = curves.first() else {
        return Err(clock_missing());
    };
    let first = *first;
    let first_start = first
        .resolved_times()
        .first()
        .copied()
        .ok_or_else(clock_missing)?;
    let first_end = first
        .resolved_times()
        .last()
        .copied()
        .ok_or_else(clock_missing)?;
    for curve in curves {
        if curve.timebase() != first.timebase()
            || curve.resolved_times().first() != Some(&first_start)
            || curve.resolved_times().last() != Some(&first_end)
        {
            return Err(text_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "validate_layer",
                "animation_range_mismatch",
                "every text visual control must share one exact clock and authored interval",
            ));
        }
    }
    Ok(())
}

fn sample_components(
    curve: &AnimationCurve,
    time: RationalTime,
    count: usize,
    field: &'static str,
) -> Result<Vec<f64>> {
    let value = curve.evaluate(time)?;
    if value.component_count() != count {
        return Err(corrupt_layout("sample_component_count_mismatch"));
    }
    let components = value.components().to_vec();
    for component in &components {
        ensure_finite(*component, field)?;
    }
    Ok(components)
}

fn sample_scalar(curve: &AnimationCurve, time: RationalTime, field: &'static str) -> Result<f64> {
    Ok(sample_components(curve, time, 1, field)?[0])
}

fn sample_discrete<T>(
    curve: &AnimationCurve,
    time: RationalTime,
    decode: impl Fn(f64) -> Option<T>,
    field: &'static str,
) -> Result<T> {
    decode(sample_scalar(curve, time, field)?).ok_or_else(|| discrete_error(field))
}

fn validate_sample(value: f64, domain: CurveDomain, field: &'static str) -> Result<()> {
    let valid = value.is_finite()
        && match domain {
            CurveDomain::Finite => value.abs() <= 1_000_000.0,
            CurveDomain::Positive => (0.0..=16_384.0).contains(&value) && value != 0.0,
            CurveDomain::Nonnegative => (0.0..=1_000_000.0).contains(&value),
            CurveDomain::Normalized => (0.0..=1.0).contains(&value),
        };
    if !valid {
        return Err(text_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "validate_control",
            "control_value_out_of_range",
            "text visual control is outside its finite supported domain",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "validate_control")
                .with_field("field", field)
                .with_field("value", value.to_string()),
        ));
    }
    Ok(())
}

fn validate_span_coverage(
    text: &str,
    ranges: impl IntoIterator<Item = TextRange>,
    kind: &'static str,
) -> Result<()> {
    let ranges = ranges.into_iter().collect::<Vec<_>>();
    if ranges.is_empty() || ranges.len() > MAX_TEXT_SPANS {
        return Err(empty_span(kind));
    }
    let mut cursor = 0;
    for range in ranges {
        validate_text_range(text, range, false, "validate_spans")?;
        if range.start != cursor {
            return Err(text_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "validate_spans",
                "span_coverage_gap_or_overlap",
                "text spans must cover the complete text exactly once in canonical order",
            ));
        }
        cursor = range.end;
    }
    if cursor != text.len() {
        return Err(text_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "validate_spans",
            "incomplete_span_coverage",
            "text spans must end at the complete UTF-8 byte length",
        ));
    }
    Ok(())
}

fn validate_paragraph_boundaries(text: &str, spans: &[ParagraphSpan]) -> Result<()> {
    for span in spans {
        let slice = &text[span.range.start..span.range.end];
        let body = slice.strip_suffix('\n').unwrap_or(slice);
        if body.contains('\n') || (span.range.end < text.len() && !slice.ends_with('\n')) {
            return Err(text_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "validate_paragraphs",
                "invalid_paragraph_boundary",
                "each paragraph span must contain one logical paragraph and its trailing newline",
            ));
        }
    }
    Ok(())
}

fn validate_text_range(
    text: &str,
    range: TextRange,
    allow_empty: bool,
    operation: &'static str,
) -> Result<()> {
    if range.end > text.len()
        || (!allow_empty && range.is_empty())
        || !text.is_char_boundary(range.start)
        || !text.is_char_boundary(range.end)
    {
        return Err(text_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            operation,
            "invalid_utf8_range",
            "text range must be in bounds and aligned to UTF-8 scalar boundaries",
        ));
    }
    Ok(())
}

fn shifted_range(span: TextRange, edit: TextRange, delta: isize) -> Result<TextRange> {
    let shift = |value: usize| -> Result<usize> {
        value
            .checked_add_signed(delta)
            .ok_or_else(|| limit_error("text_byte_limit", usize::MAX, MAX_TEXT_BYTES))
    };
    if span.end <= edit.start {
        Ok(span)
    } else if edit.end <= span.start {
        TextRange::new(shift(span.start)?, shift(span.end)?)
    } else {
        TextRange::new(span.start, shift(span.end)?)
    }
}

fn validate_label(
    value: &str,
    maximum: usize,
    field: &'static str,
    operation: &'static str,
) -> Result<()> {
    if value.trim().is_empty() || value.len() > maximum || value.chars().any(char::is_control) {
        return Err(text_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            operation,
            "invalid_text_label",
            "text label must be nonblank, bounded UTF-8 without control characters",
        )
        .with_context(ErrorContext::new(COMPONENT, operation).with_field("field", field)));
    }
    Ok(())
}

fn validate_tag(tag: [u8; 4], operation: &'static str) -> Result<()> {
    if !tag.iter().all(|byte| (0x20..=0x7e).contains(byte)) {
        return Err(text_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            operation,
            "invalid_opentype_tag",
            "OpenType tag must contain four printable ASCII bytes",
        ));
    }
    Ok(())
}

fn collect_bounded<T>(
    values: impl IntoIterator<Item = T>,
    limit: usize,
    reason: &'static str,
) -> Result<Vec<T>> {
    let mut incoming = values.into_iter();
    let mut collected = Vec::with_capacity(incoming.size_hint().0.min(limit));
    for _ in 0..=limit {
        let Some(value) = incoming.next() else {
            return Ok(collected);
        };
        collected.push(value);
    }
    Err(limit_error(reason, collected.len(), limit))
}

fn ensure_finite(value: f64, field: &'static str) -> Result<()> {
    if !value.is_finite() {
        return Err(corrupt_layout("nonfinite_shaping_output")
            .with_context(ErrorContext::new(COMPONENT, "shape_text").with_field("field", field)));
    }
    Ok(())
}

fn duplicate_setting(kind: &'static str) -> Error {
    text_error(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        "validate_style",
        "duplicate_font_setting",
        "font feature and variation tags must be unique in their namespaces",
    )
    .with_context(ErrorContext::new(COMPONENT, "validate_style").with_field("kind", kind))
}

fn discrete_error(field: &'static str) -> Error {
    text_error(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        "validate_discrete_control",
        "invalid_discrete_animation",
        "discrete text controls require exact supported values and hold interpolation when changing",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "validate_discrete_control").with_field("field", field),
    )
}

fn empty_span(kind: &'static str) -> Error {
    text_error(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        "create_span",
        "empty_text_span",
        "text style and paragraph spans must be nonempty",
    )
    .with_context(ErrorContext::new(COMPONENT, "create_span").with_field("kind", kind))
}

fn clock_missing() -> Error {
    text_error(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        "validate_layer",
        "missing_animation_clock",
        "text layer must contain at least one animated visual control",
    )
}

fn invalid_font(font: &FontFace) -> Error {
    text_error(
        ErrorCategory::CorruptData,
        Recoverability::UserCorrectable,
        "resolve_font",
        "invalid_font_bytes",
        "resolved font bytes do not contain the persisted collection face",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "resolve_font")
            .with_field("asset_id", font.asset_id())
            .with_field("collection_index", font.collection_index().to_string()),
    )
}

fn corrupt_layout(reason: &'static str) -> Error {
    text_error(
        ErrorCategory::CorruptData,
        Recoverability::UserCorrectable,
        "shape_text",
        reason,
        "font shaping produced state that violates the checked text layout contract",
    )
}

fn limit_error(reason: &'static str, actual: usize, limit: usize) -> Error {
    text_error(
        ErrorCategory::ResourceExhausted,
        Recoverability::UserCorrectable,
        "validate_text_limits",
        reason,
        "text state exceeds a supported deterministic resource bound",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "validate_text_limits")
            .with_field("actual", actual.to_string())
            .with_field("limit", limit.to_string()),
    )
}

fn text_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    reason: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation).with_field("reason", reason))
}
