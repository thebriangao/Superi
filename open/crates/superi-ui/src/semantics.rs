//! Stable semantic output derived from the retained scene.

use std::collections::BTreeMap;

use accesskit::{
    Action as AccessAction, Node as AccessNode, NodeId as AccessNodeId, Rect as AccessRect,
    Role as AccessRole, Tree as AccessTree, TreeId, TreeUpdate,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::scene::{NodeId, Rect, Scene};

/// Platform-neutral semantic role.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SemanticRole {
    Window,
    Group,
    Toolbar,
    Button,
    Tab,
    Text,
    Image,
    Slider,
    Timeline,
    Status,
    Search,
}

/// Actions supported by a semantic node.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticActions {
    pub(crate) activate: bool,
    pub(crate) increment: bool,
    pub(crate) decrement: bool,
    pub(crate) focus: bool,
}

impl SemanticActions {
    /// Returns whether activation is supported.
    #[must_use]
    pub const fn activate(self) -> bool {
        self.activate
    }

    /// Returns whether increment is supported.
    #[must_use]
    pub const fn increment(self) -> bool {
        self.increment
    }

    /// Returns whether decrement is supported.
    #[must_use]
    pub const fn decrement(self) -> bool {
        self.decrement
    }

    /// Returns whether focus is supported.
    #[must_use]
    pub const fn focus(self) -> bool {
        self.focus
    }
}

/// One semantic node emitted from one retained node.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticNode {
    id: NodeId,
    role: SemanticRole,
    name: String,
    value: Option<String>,
    description: Option<String>,
    bounds: Rect,
    selected: bool,
    focused: bool,
    disabled: bool,
    focus_order: Option<u32>,
    actions: SemanticActions,
}

impl SemanticNode {
    /// Returns stable retained identity.
    #[must_use]
    pub const fn id(&self) -> &NodeId {
        &self.id
    }

    /// Returns platform-neutral role.
    #[must_use]
    pub const fn role(&self) -> SemanticRole {
        self.role
    }

    /// Returns human-facing accessible name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns current semantic value.
    #[must_use]
    pub fn value(&self) -> Option<&str> {
        self.value.as_deref()
    }

    /// Returns current logical bounds.
    #[must_use]
    pub const fn bounds(&self) -> Rect {
        self.bounds
    }

    /// Returns whether selected.
    #[must_use]
    pub const fn selected(&self) -> bool {
        self.selected
    }

    /// Returns whether focused.
    #[must_use]
    pub const fn focused(&self) -> bool {
        self.focused
    }

    /// Returns supported actions.
    #[must_use]
    pub const fn actions(&self) -> SemanticActions {
        self.actions
    }
}

/// Ordered semantic tree emitted atomically from one scene.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SemanticTree {
    root: NodeId,
    focused: Option<NodeId>,
    nodes: Vec<SemanticNode>,
}

impl SemanticTree {
    pub(crate) fn from_scene(scene: &Scene) -> Self {
        let mut focus_index = 0_u32;
        let nodes = scene
            .nodes()
            .iter()
            .filter_map(|node| {
                let semantic = node.semantic()?;
                let order = if node.is_focusable() {
                    let current = focus_index;
                    focus_index = focus_index.saturating_add(1);
                    Some(current)
                } else {
                    None
                };
                let mut actions = semantic.actions;
                actions.focus |= node.is_focusable();
                Some(SemanticNode {
                    id: node.id().clone(),
                    role: semantic.role,
                    name: semantic.name.clone(),
                    value: semantic.value.clone(),
                    description: semantic.description.clone(),
                    bounds: node.hit_bounds(),
                    selected: semantic.selected,
                    focused: scene.focused() == Some(node.id()),
                    disabled: semantic.disabled,
                    focus_order: order,
                    actions,
                })
            })
            .collect();
        Self {
            root: NodeId::new("application").expect("static root identity is valid"),
            focused: scene.focused().cloned(),
            nodes,
        }
    }

    /// Returns the semantic root identity.
    #[must_use]
    pub const fn root(&self) -> &NodeId {
        &self.root
    }

    /// Returns semantic focus.
    #[must_use]
    pub const fn focused(&self) -> Option<&NodeId> {
        self.focused.as_ref()
    }

    /// Returns semantic nodes in retained order.
    #[must_use]
    pub fn nodes(&self) -> &[SemanticNode] {
        &self.nodes
    }

    /// Finds one semantic node by stable identity.
    #[must_use]
    pub fn node(&self, id: &NodeId) -> Option<&SemanticNode> {
        self.nodes.iter().find(|node| node.id == *id)
    }

    /// Converts the same retained semantic snapshot into one atomic AccessKit update.
    pub fn accesskit_update(&self) -> Result<TreeUpdate, SemanticBridgeError> {
        let mut identities = BTreeMap::<AccessNodeId, &str>::new();
        for node in &self.nodes {
            let access_id = accesskit_id(node.id());
            if let Some(existing) = identities.insert(access_id, node.id().as_str()) {
                return Err(SemanticBridgeError::IdentityCollision {
                    first: existing.to_owned(),
                    second: node.id().to_string(),
                });
            }
        }

        let root_id = accesskit_id(&self.root);
        let child_ids = self
            .nodes
            .iter()
            .filter(|node| node.id != self.root)
            .map(|node| accesskit_id(node.id()))
            .collect::<Vec<_>>();
        let mut nodes = self
            .nodes
            .iter()
            .map(|semantic| {
                let mut node = accesskit_node(semantic);
                if semantic.id == self.root {
                    node.set_children(child_ids.clone());
                }
                (accesskit_id(semantic.id()), node)
            })
            .collect::<Vec<_>>();

        if !self.nodes.iter().any(|node| node.id == self.root) {
            let mut root = AccessNode::new(AccessRole::Application);
            root.set_label("Superi");
            root.set_children(child_ids);
            nodes.insert(0, (root_id, root));
        }

        let mut tree = AccessTree::new(root_id);
        tree.toolkit_name = Some("Superi retained UI".to_owned());
        tree.toolkit_version = Some(env!("CARGO_PKG_VERSION").to_owned());
        Ok(TreeUpdate {
            nodes,
            tree: Some(tree),
            tree_id: TreeId::ROOT,
            focus: self.focused.as_ref().map_or(root_id, accesskit_id),
        })
    }
}

/// Conversion failure that prevents publication of an ambiguous native semantic tree.
#[derive(Debug, thiserror::Error)]
pub enum SemanticBridgeError {
    #[error("semantic identities `{first}` and `{second}` collide in the native bridge")]
    IdentityCollision { first: String, second: String },
}

/// Derives a stable process-independent AccessKit identity from one retained identity.
#[must_use]
pub fn accesskit_id(id: &NodeId) -> AccessNodeId {
    let digest = Sha256::digest(id.as_str().as_bytes());
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&digest[..8]);
    AccessNodeId(u64::from_be_bytes(bytes))
}

fn accesskit_node(semantic: &SemanticNode) -> AccessNode {
    let mut node = AccessNode::new(accesskit_role(semantic.role));
    if semantic.role == SemanticRole::Text {
        node.set_value(semantic.name.clone());
    } else {
        node.set_label(semantic.name.clone());
    }
    node.set_author_id(semantic.id.to_string());
    if let Some(value) = &semantic.value {
        node.set_value(value.clone());
    }
    if let Some(description) = &semantic.description {
        node.set_description(description.clone());
    }
    let bounds = semantic.bounds;
    node.set_bounds(AccessRect {
        x0: f64::from(bounds.x),
        y0: f64::from(bounds.y),
        x1: f64::from(bounds.x + bounds.width),
        y1: f64::from(bounds.y + bounds.height),
    });
    if semantic.selected {
        node.set_selected(true);
    }
    if semantic.disabled {
        node.set_disabled();
    }
    if semantic.actions.activate {
        node.add_action(AccessAction::Click);
    }
    if semantic.actions.focus {
        node.add_action(AccessAction::Focus);
    }
    if semantic.actions.increment {
        node.add_action(AccessAction::Increment);
    }
    if semantic.actions.decrement {
        node.add_action(AccessAction::Decrement);
    }
    node
}

const fn accesskit_role(role: SemanticRole) -> AccessRole {
    match role {
        SemanticRole::Window => AccessRole::Window,
        SemanticRole::Group => AccessRole::Group,
        SemanticRole::Toolbar => AccessRole::Toolbar,
        SemanticRole::Button => AccessRole::Button,
        SemanticRole::Tab => AccessRole::Tab,
        SemanticRole::Text => AccessRole::Label,
        SemanticRole::Image => AccessRole::Image,
        SemanticRole::Slider => AccessRole::Slider,
        SemanticRole::Timeline => AccessRole::Canvas,
        SemanticRole::Status => AccessRole::Status,
        SemanticRole::Search => AccessRole::SearchInput,
    }
}
