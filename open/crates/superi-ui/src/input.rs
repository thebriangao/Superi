//! Deterministic input normalization and foundation interaction state.

use serde::{Deserialize, Serialize};

use crate::fixture::{FoundationProbe, FoundationState};
use crate::scene::{NodeId, Scene};
use crate::{Result, UiError};

/// Logical key command after platform normalization.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Key {
    Tab,
    ShiftTab,
    Enter,
    Space,
    Escape,
    ArrowLeft,
    ArrowRight,
}

/// Normalized input routed through the retained scene.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum InputEvent {
    Activate(NodeId),
    Focus(NodeId),
    Pointer { x: f32, y: f32 },
    Key(Key),
    Text(String),
}

/// One deterministic input transcript entry.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputTranscriptEntry {
    sequence: u64,
    event: InputEvent,
    target: Option<NodeId>,
    outcome: String,
}

/// Owns only ephemeral fixture interaction state.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InteractionController {
    state: FoundationState,
    transcript: Vec<InputTranscriptEntry>,
    next_sequence: u64,
}

impl InteractionController {
    /// Creates a controller around noncanonical presentation state.
    #[must_use]
    pub const fn new(state: FoundationState) -> Self {
        Self {
            state,
            transcript: Vec::new(),
            next_sequence: 1,
        }
    }

    /// Returns current ephemeral state.
    #[must_use]
    pub const fn state(&self) -> &FoundationState {
        &self.state
    }

    /// Returns deterministic transcript.
    #[must_use]
    pub fn transcript(&self) -> &[InputTranscriptEntry] {
        &self.transcript
    }

    /// Dispatches one normalized event against an immutable scene snapshot.
    pub fn dispatch(&mut self, scene: &Scene, event: InputEvent) -> Result<()> {
        let target = match &event {
            InputEvent::Activate(id) | InputEvent::Focus(id) => Some(id.clone()),
            InputEvent::Pointer { x, y } => scene.hit_test(*x, *y).cloned(),
            InputEvent::Key(Key::Enter | Key::Space) => self.state.focused.clone(),
            InputEvent::Key(_) | InputEvent::Text(_) => None,
        };
        let outcome = match &event {
            InputEvent::Activate(id) => self.activate(scene, id)?,
            InputEvent::Focus(id) => self.focus(scene, id)?,
            InputEvent::Pointer { .. } => match target.as_ref() {
                Some(id) => self.activate(scene, id)?,
                None => "pointer missed every interactive node".to_owned(),
            },
            InputEvent::Key(Key::Tab) => self.move_focus(scene, 1)?,
            InputEvent::Key(Key::ShiftTab) => self.move_focus(scene, -1)?,
            InputEvent::Key(Key::Enter | Key::Space) => match target.as_ref() {
                Some(id) => self.activate(scene, id)?,
                None => "no focused action".to_owned(),
            },
            InputEvent::Key(Key::Escape) => {
                self.state.focused = None;
                "focus cleared".to_owned()
            }
            InputEvent::Key(Key::ArrowLeft) => {
                let probe = self.state.selected_probe.previous();
                self.state.selected_probe = probe;
                self.state.focused = Some(NodeId::new(probe.node_id())?);
                format!("foundation probe moved to `{}`", probe.node_id())
            }
            InputEvent::Key(Key::ArrowRight) => {
                let probe = self.state.selected_probe.next();
                self.state.selected_probe = probe;
                self.state.focused = Some(NodeId::new(probe.node_id())?);
                format!("foundation probe moved to `{}`", probe.node_id())
            }
            InputEvent::Text(text) => {
                if self.state.text_sample.chars().count() + text.chars().count() > 128 {
                    return Err(UiError::Invalid(
                        "foundation text sample exceeds 128 characters".to_owned(),
                    ));
                }
                self.state.text_sample.push_str(text);
                "foundation text sample updated".to_owned()
            }
        };
        let sequence = self.next_sequence;
        self.next_sequence = self
            .next_sequence
            .checked_add(1)
            .ok_or_else(|| UiError::Invalid("input transcript sequence is exhausted".to_owned()))?;
        self.transcript.push(InputTranscriptEntry {
            sequence,
            event,
            target,
            outcome,
        });
        Ok(())
    }

    fn activate(&mut self, scene: &Scene, id: &NodeId) -> Result<String> {
        let node = scene.node(id).ok_or_else(|| {
            UiError::Invalid(format!("input target `{id}` is not in the retained scene"))
        })?;
        let semantic = node.semantic().ok_or_else(|| {
            UiError::Invalid(format!("input target `{id}` has no semantic action"))
        })?;
        if semantic.disabled {
            return Ok(format!("`{id}` is disabled"));
        }
        if !semantic.actions.activate() {
            return Err(UiError::Invalid(format!(
                "input target `{id}` does not support activation"
            )));
        }
        self.state.focused = Some(id.clone());
        match id.as_str() {
            "foundation.scene" => self.state.selected_probe = FoundationProbe::Scene,
            "foundation.input" => self.state.selected_probe = FoundationProbe::Input,
            "foundation.semantics" => self.state.selected_probe = FoundationProbe::Semantics,
            "foundation.capture" => self.state.selected_probe = FoundationProbe::Capture,
            _ => {}
        }
        Ok(format!("activated `{id}`"))
    }

    fn focus(&mut self, scene: &Scene, id: &NodeId) -> Result<String> {
        let node = scene.node(id).ok_or_else(|| {
            UiError::Invalid(format!("focus target `{id}` is not in the retained scene"))
        })?;
        if !node.is_focusable() {
            return Err(UiError::Invalid(format!(
                "focus target `{id}` is not focusable"
            )));
        }
        self.state.focused = Some(id.clone());
        Ok(format!("focused `{id}`"))
    }

    fn move_focus(&mut self, scene: &Scene, direction: i32) -> Result<String> {
        let order = scene.focus_order();
        if order.is_empty() {
            return Err(UiError::Unavailable(
                "retained scene has no focusable nodes".to_owned(),
            ));
        }
        let current = self
            .state
            .focused
            .as_ref()
            .and_then(|focused| order.iter().position(|id| *id == focused));
        let next = match (current, direction) {
            (Some(index), value) if value < 0 => {
                if index == 0 {
                    order.len() - 1
                } else {
                    index - 1
                }
            }
            (Some(index), _) => (index + 1) % order.len(),
            (None, value) if value < 0 => order.len() - 1,
            (None, _) => 0,
        };
        self.state.focused = Some(order[next].clone());
        Ok(format!("focus moved to `{}`", order[next]))
    }
}
