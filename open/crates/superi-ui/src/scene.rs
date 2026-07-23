//! Stable retained nodes shared by drawing, hit testing, focus, and semantics.

use std::collections::BTreeSet;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::semantics::{SemanticActions, SemanticRole, SemanticTree};
use crate::{Result, UiError};

/// Stable identity for one retained interface node.
#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
#[serde(transparent)]
pub struct NodeId(String);

impl NodeId {
    /// Validates and creates a dotted lowercase node identity.
    pub fn new(value: impl Into<String>) -> Result<Self> {
        let value = value.into();
        let valid = !value.is_empty()
            && value.len() <= 128
            && value
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'.');
        if !valid || value.starts_with('.') || value.ends_with('.') || value.contains("..") {
            return Err(UiError::Invalid(format!(
                "node identity `{value}` must use dotted lowercase ASCII"
            )));
        }
        Ok(Self(value))
    }

    /// Returns the stable serialized identity.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for NodeId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Logical axis-aligned bounds.
#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

impl Rect {
    /// Creates finite positive bounds.
    pub fn new(x: f32, y: f32, width: f32, height: f32) -> Result<Self> {
        if ![x, y, width, height].into_iter().all(f32::is_finite) || width < 0.0 || height < 0.0 {
            return Err(UiError::Invalid(
                "retained bounds must be finite and nonnegative".to_owned(),
            ));
        }
        Ok(Self {
            x,
            y,
            width,
            height,
        })
    }

    /// Returns whether a logical point lies inside these half-open bounds.
    #[must_use]
    pub fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }

    /// Intersects two rectangles.
    #[must_use]
    pub fn intersection(self, other: Self) -> Option<Self> {
        let x = self.x.max(other.x);
        let y = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);
        (right > x && bottom > y).then_some(Self {
            x,
            y,
            width: right - x,
            height: bottom - y,
        })
    }
}

/// Unpremultiplied eight-bit RGBA color.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Color(pub [u8; 4]);

impl Color {
    pub const BLACK: Self = Self([0, 0, 0, 255]);
    pub const WHITE: Self = Self([241, 244, 248, 255]);
    pub const MUTED: Self = Self([125, 134, 148, 255]);
    pub const SEAM: Self = Self([36, 40, 48, 255]);
    pub const CYAN: Self = Self([50, 219, 255, 255]);
    pub const VIOLET: Self = Self([153, 112, 255, 255]);
    pub const GREEN: Self = Self([91, 227, 159, 255]);
    pub const AMBER: Self = Self([255, 190, 92, 255]);
    pub const RED: Self = Self([255, 97, 116, 255]);
}

/// Draw payload for one retained node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NodeKind {
    Rect {
        fill: Color,
    },
    Stroke {
        color: Color,
        width: f32,
    },
    Text {
        text: String,
        size: f32,
        weight: u16,
        color: Color,
    },
    Icon {
        name: String,
        color: Color,
    },
}

/// Semantic metadata retained beside the visual node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticSpec {
    pub role: SemanticRole,
    pub name: String,
    pub value: Option<String>,
    pub description: Option<String>,
    pub selected: bool,
    pub disabled: bool,
    pub actions: SemanticActions,
}

impl SemanticSpec {
    /// Creates a named semantic node with no state or action.
    #[must_use]
    pub fn new(role: SemanticRole, name: impl Into<String>) -> Self {
        Self {
            role,
            name: name.into(),
            value: None,
            description: None,
            selected: false,
            disabled: false,
            actions: SemanticActions::default(),
        }
    }

    /// Marks the semantic node as activatable.
    #[must_use]
    pub const fn activatable(mut self) -> Self {
        self.actions.activate = true;
        self
    }

    /// Marks the semantic node selected.
    #[must_use]
    pub const fn selected(mut self, selected: bool) -> Self {
        self.selected = selected;
        self
    }

    /// Adds a current value.
    #[must_use]
    pub fn with_value(mut self, value: impl Into<String>) -> Self {
        self.value = Some(value.into());
        self
    }

    /// Adds a useful description.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

/// One immutable retained visual, input, and semantic node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    id: NodeId,
    bounds: Rect,
    hit_bounds: Rect,
    clip: Option<Rect>,
    z: i32,
    visible: bool,
    focusable: bool,
    kind: NodeKind,
    semantic: Option<SemanticSpec>,
}

impl Node {
    /// Creates one visual node with visual bounds as its hit bounds.
    #[must_use]
    pub fn new(id: NodeId, bounds: Rect, z: i32, kind: NodeKind) -> Self {
        Self {
            id,
            bounds,
            hit_bounds: bounds,
            clip: None,
            z,
            visible: true,
            focusable: false,
            kind,
            semantic: None,
        }
    }

    /// Adds semantic metadata.
    #[must_use]
    pub fn with_semantics(mut self, semantic: SemanticSpec) -> Self {
        self.semantic = Some(semantic);
        self
    }

    /// Makes the node part of deterministic focus traversal.
    #[must_use]
    pub const fn focusable(mut self) -> Self {
        self.focusable = true;
        self
    }

    /// Expands or replaces the pointer hit region.
    #[must_use]
    pub const fn with_hit_bounds(mut self, hit_bounds: Rect) -> Self {
        self.hit_bounds = hit_bounds;
        self
    }

    /// Clips drawing and hit testing to a retained rectangle.
    #[must_use]
    pub const fn with_clip(mut self, clip: Rect) -> Self {
        self.clip = Some(clip);
        self
    }

    /// Returns stable identity.
    #[must_use]
    pub const fn id(&self) -> &NodeId {
        &self.id
    }

    /// Returns logical visual bounds.
    #[must_use]
    pub const fn bounds(&self) -> Rect {
        self.bounds
    }

    /// Returns pointer hit bounds.
    #[must_use]
    pub const fn hit_bounds(&self) -> Rect {
        self.hit_bounds
    }

    /// Returns retained clipping.
    #[must_use]
    pub const fn clip(&self) -> Option<Rect> {
        self.clip
    }

    /// Returns deterministic draw order.
    #[must_use]
    pub const fn z(&self) -> i32 {
        self.z
    }

    /// Returns whether drawing and hit testing are active.
    #[must_use]
    pub const fn visible(&self) -> bool {
        self.visible
    }

    /// Returns whether the node participates in focus traversal.
    #[must_use]
    pub const fn is_focusable(&self) -> bool {
        self.focusable
    }

    /// Returns draw payload.
    #[must_use]
    pub const fn kind(&self) -> &NodeKind {
        &self.kind
    }

    /// Returns semantic metadata when exposed.
    #[must_use]
    pub const fn semantic(&self) -> Option<&SemanticSpec> {
        self.semantic.as_ref()
    }
}

/// Immutable retained scene used by every interface consumer.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Scene {
    logical_width: u32,
    logical_height: u32,
    scale_factor: f32,
    nodes: Vec<Node>,
    focused: Option<NodeId>,
}

impl Scene {
    /// Validates a complete scene snapshot.
    pub fn new(
        logical_width: u32,
        logical_height: u32,
        scale_factor: f32,
        nodes: Vec<Node>,
        focused: Option<NodeId>,
    ) -> Result<Self> {
        if logical_width == 0
            || logical_height == 0
            || !scale_factor.is_finite()
            || scale_factor <= 0.0
        {
            return Err(UiError::Invalid(
                "scene dimensions and scale must be positive".to_owned(),
            ));
        }
        let mut identities = BTreeSet::new();
        for node in &nodes {
            if !identities.insert(node.id.clone()) {
                return Err(UiError::Invalid(format!(
                    "retained node identity `{}` is duplicated",
                    node.id
                )));
            }
            if node.focusable && node.semantic.is_none() {
                return Err(UiError::Invalid(format!(
                    "focusable node `{}` requires semantic metadata",
                    node.id
                )));
            }
            if node.focusable && (node.hit_bounds.width < 24.0 || node.hit_bounds.height < 24.0) {
                return Err(UiError::Invalid(format!(
                    "focusable node `{}` has a hit target smaller than 24 by 24",
                    node.id
                )));
            }
        }
        if focused
            .as_ref()
            .is_some_and(|id| !nodes.iter().any(|node| node.id == *id && node.focusable))
        {
            return Err(UiError::Invalid(
                "focused identity is not a focusable scene node".to_owned(),
            ));
        }
        Ok(Self {
            logical_width,
            logical_height,
            scale_factor,
            nodes,
            focused,
        })
    }

    /// Returns logical width.
    #[must_use]
    pub const fn logical_width(&self) -> u32 {
        self.logical_width
    }

    /// Returns logical height.
    #[must_use]
    pub const fn logical_height(&self) -> u32 {
        self.logical_height
    }

    /// Returns pinned display scale.
    #[must_use]
    pub const fn scale_factor(&self) -> f32 {
        self.scale_factor
    }

    /// Returns checked physical output width.
    pub fn physical_width(&self) -> Result<u32> {
        scaled_dimension(self.logical_width, self.scale_factor)
    }

    /// Returns checked physical output height.
    pub fn physical_height(&self) -> Result<u32> {
        scaled_dimension(self.logical_height, self.scale_factor)
    }

    /// Returns retained nodes in insertion order.
    #[must_use]
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// Finds one node by stable identity.
    #[must_use]
    pub fn node(&self, id: &NodeId) -> Option<&Node> {
        self.nodes.iter().find(|node| node.id == *id)
    }

    /// Returns the currently focused retained node.
    #[must_use]
    pub const fn focused(&self) -> Option<&NodeId> {
        self.focused.as_ref()
    }

    /// Returns deterministic focus order.
    #[must_use]
    pub fn focus_order(&self) -> Vec<&NodeId> {
        self.nodes
            .iter()
            .filter(|node| node.visible && node.focusable)
            .map(Node::id)
            .collect()
    }

    /// Hit tests a logical point in descending draw order.
    #[must_use]
    pub fn hit_test(&self, x: f32, y: f32) -> Option<&NodeId> {
        self.nodes
            .iter()
            .enumerate()
            .filter(|(_, node)| {
                node.visible
                    && node.semantic.is_some()
                    && node.hit_bounds.contains(x, y)
                    && node.clip.map_or(true, |clip| clip.contains(x, y))
            })
            .max_by_key(|(index, node)| (node.z, *index))
            .map(|(_, node)| node.id())
    }

    /// Proves that an identity resolves through the same hit-testable scene.
    #[must_use]
    pub fn hit_test_node(&self, id: &NodeId) -> Option<NodeId> {
        self.node(id)
            .filter(|node| node.visible && node.semantic.is_some())
            .map(|node| node.id.clone())
    }

    /// Builds semantic output from this exact scene snapshot.
    #[must_use]
    pub fn semantics(&self) -> SemanticTree {
        SemanticTree::from_scene(self)
    }
}

fn scaled_dimension(logical: u32, scale: f32) -> Result<u32> {
    let value = logical as f64 * f64::from(scale);
    if !value.is_finite() || value > f64::from(u32::MAX) {
        return Err(UiError::Invalid(
            "scaled scene dimension is exhausted".to_owned(),
        ));
    }
    Ok(value.round().max(1.0) as u32)
}
