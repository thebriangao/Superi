//! Deterministic Phase Infinity scaffold diagnostic.

use serde::{Deserialize, Serialize};

use crate::scene::{Color, Node, NodeId, NodeKind, Rect, Scene, SemanticSpec};
use crate::semantics::SemanticRole;
use crate::{Result, UiError};

const PLANE_RAISED: Color = Color([10, 12, 16, 255]);
const PLANE_ACTIVE: Color = Color([17, 21, 28, 255]);
const PLANE_SELECTED: Color = Color([22, 28, 37, 255]);
const GRID: Color = Color([24, 28, 35, 255]);
const CYAN_SOFT: Color = Color([18, 66, 78, 255]);
const VIOLET_SOFT: Color = Color([48, 38, 76, 255]);
const GREEN_SOFT: Color = Color([24, 67, 51, 255]);
const AMBER_SOFT: Color = Color([75, 54, 26, 255]);

/// Scaffold capability selected for deterministic interaction proof.
#[derive(Clone, Copy, Debug, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FoundationProbe {
    #[default]
    Scene,
    Input,
    Semantics,
    Capture,
}

impl FoundationProbe {
    pub(crate) const fn node_id(self) -> &'static str {
        match self {
            Self::Scene => "foundation.scene",
            Self::Input => "foundation.input",
            Self::Semantics => "foundation.semantics",
            Self::Capture => "foundation.capture",
        }
    }

    pub(crate) const fn previous(self) -> Self {
        match self {
            Self::Scene => Self::Capture,
            Self::Input => Self::Scene,
            Self::Semantics => Self::Input,
            Self::Capture => Self::Semantics,
        }
    }

    pub(crate) const fn next(self) -> Self {
        match self {
            Self::Scene => Self::Input,
            Self::Input => Self::Semantics,
            Self::Semantics => Self::Capture,
            Self::Capture => Self::Scene,
        }
    }

    const fn label(self) -> &'static str {
        match self {
            Self::Scene => "RETAINED SCENE",
            Self::Input => "NORMALIZED INPUT",
            Self::Semantics => "SEMANTIC PROJECTION",
            Self::Capture => "PRIVATE CAPTURE",
        }
    }

    const fn accessible_name(self) -> &'static str {
        match self {
            Self::Scene => "Retained scene probe",
            Self::Input => "Normalized input probe",
            Self::Semantics => "Semantic projection probe",
            Self::Capture => "Private capture probe",
        }
    }

    const fn detail(self) -> &'static str {
        match self {
            Self::Scene => "ONE IMMUTABLE TREE DRIVES DRAW, HIT, FOCUS, AND ORDER",
            Self::Input => "POINTER, KEY, TEXT, AND ACCESSIBILITY ENTER ONE ROUTE",
            Self::Semantics => "STABLE NODE IDENTITY PROJECTS ONE ATOMIC ACCESSKIT TREE",
            Self::Capture => "PRODUCT WGPU PATH EMITS PIXELS, SEMANTICS, AND TRANSCRIPT",
        }
    }

    const fn metric(self) -> &'static str {
        match self {
            Self::Scene => "STABLE IDS / DETERMINISTIC ORDER",
            Self::Input => "FOCUS / ACTIVATE / TEXT / KEYS",
            Self::Semantics => "ROLE / NAME / STATE / ACTION",
            Self::Capture => "PNG / JSON / HASH / REPLAY",
        }
    }

    const fn accent(self) -> Color {
        match self {
            Self::Scene => Color::CYAN,
            Self::Input => Color::VIOLET,
            Self::Semantics => Color::GREEN,
            Self::Capture => Color::AMBER,
        }
    }

    const fn soft(self) -> Color {
        match self {
            Self::Scene => CYAN_SOFT,
            Self::Input => VIOLET_SOFT,
            Self::Semantics => GREEN_SOFT,
            Self::Capture => AMBER_SOFT,
        }
    }
}

/// Ephemeral state used only by the scaffold diagnostic.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FoundationState {
    pub(crate) selected_probe: FoundationProbe,
    pub(crate) focused: Option<NodeId>,
    pub(crate) text_sample: String,
}

impl FoundationState {
    /// Returns current semantic focus.
    #[must_use]
    pub const fn focused(&self) -> Option<&NodeId> {
        self.focused.as_ref()
    }

    /// Returns the selected foundation probe.
    #[must_use]
    pub const fn selected_probe(&self) -> FoundationProbe {
        self.selected_probe
    }
}

impl Default for FoundationState {
    fn default() -> Self {
        Self {
            selected_probe: FoundationProbe::Scene,
            focused: Some(id("foundation.scene")),
            text_sample: "INTER 4.1 / RETAINED TEXT".to_owned(),
        }
    }
}

/// Pinned dimensions and scale for the neutral native scaffold.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FoundationFixture {
    logical_width: u32,
    logical_height: u32,
    scale_factor: f32,
}

impl FoundationFixture {
    /// Creates a fixture large enough to preserve the scaffold hierarchy.
    pub fn new(logical_width: u32, logical_height: u32, scale_factor: f32) -> Result<Self> {
        if logical_width < 960
            || logical_height < 640
            || !scale_factor.is_finite()
            || scale_factor <= 0.0
            || scale_factor > 4.0
        {
            return Err(UiError::Invalid(
                "foundation fixture requires at least 960 by 640 and scale from 0 to 4".to_owned(),
            ));
        }
        Ok(Self {
            logical_width,
            logical_height,
            scale_factor,
        })
    }

    /// Builds one immutable retained scaffold scene from ephemeral diagnostic state.
    pub fn scene(&self, state: &FoundationState) -> Result<Scene> {
        let width = self.logical_width as f32;
        let height = self.logical_height as f32;
        let header_h = 58.0;
        let status_h = 34.0;
        let left_w = (width * 0.21).clamp(246.0, 302.0);
        let right_w = (width * 0.235).clamp(280.0, 344.0);
        let body_y = header_h;
        let body_h = height - header_h - status_h;
        let center_x = left_w;
        let center_w = width - left_w - right_w;
        let right_x = width - right_w;
        let mut nodes = Vec::new();

        nodes.push(
            Node::new(
                id("application"),
                rect(0.0, 0.0, width, height),
                0,
                NodeKind::Rect { fill: Color::BLACK },
            )
            .with_semantics(
                SemanticSpec::new(SemanticRole::Window, "Superi Phase Infinity scaffold")
                    .with_description(
                        "Native foundation diagnostic only; product surfaces are deferred",
                    ),
            ),
        );
        nodes.push(Node::new(
            id("application.signal"),
            rect(0.0, 0.0, 3.0, height),
            40,
            NodeKind::Rect {
                fill: state.selected_probe.accent(),
            },
        ));

        panel(
            &mut nodes,
            "header",
            "Scaffold header",
            rect(3.0, 0.0, width - 3.0, header_h),
            2,
            PLANE_RAISED,
        );
        seam(
            &mut nodes,
            "header.seam",
            3.0,
            header_h - 1.0,
            width - 3.0,
            1.0,
            4,
        );
        text(
            &mut nodes,
            "header.brand",
            "SUPERI",
            rect(20.0, 17.0, 78.0, 20.0),
            14.0,
            690,
            Color::WHITE,
            10,
        );
        text(
            &mut nodes,
            "header.phase",
            "PHASE INFINITY / PROGRAM SCAFFOLD",
            rect(116.0, 19.0, 274.0, 18.0),
            10.0,
            570,
            Color::MUTED,
            10,
        );
        text(
            &mut nodes,
            "header.boundary",
            "FOUNDATION ONLY",
            rect(width - 154.0, 19.0, 134.0, 18.0),
            10.0,
            650,
            state.selected_probe.accent(),
            10,
        );

        panel(
            &mut nodes,
            "foundation.layers",
            "Foundation capability probes",
            rect(3.0, body_y, left_w - 3.0, body_h),
            2,
            PLANE_RAISED,
        );
        seam(
            &mut nodes,
            "foundation.layers.seam",
            left_w - 1.0,
            body_y,
            1.0,
            body_h,
            4,
        );
        section_label(
            &mut nodes,
            "foundation.layers.label",
            "PLATFORM CAPABILITIES",
            20.0,
            body_y + 20.0,
        );
        text(
            &mut nodes,
            "foundation.layers.note",
            "REAL PATHS, NO PRODUCT WORKSPACE",
            rect(20.0, body_y + 43.0, left_w - 40.0, 15.0),
            8.5,
            520,
            Color::MUTED,
            6,
        );

        for (index, probe) in [
            FoundationProbe::Scene,
            FoundationProbe::Input,
            FoundationProbe::Semantics,
            FoundationProbe::Capture,
        ]
        .into_iter()
        .enumerate()
        {
            foundation_probe(
                &mut nodes,
                probe,
                rect(
                    12.0,
                    body_y + 78.0 + index as f32 * 68.0,
                    left_w - 24.0,
                    54.0,
                ),
                state.selected_probe == probe,
            );
        }

        text(
            &mut nodes,
            "foundation.contract.label",
            "C001 CONTRACT",
            rect(20.0, body_y + body_h - 108.0, left_w - 40.0, 15.0),
            9.0,
            650,
            Color::MUTED,
            7,
        );
        text(
            &mut nodes,
            "foundation.contract.line.one",
            "BUILD THE PLATFORM",
            rect(20.0, body_y + body_h - 80.0, left_w - 40.0, 16.0),
            10.0,
            620,
            Color::WHITE,
            7,
        );
        text(
            &mut nodes,
            "foundation.contract.line.two",
            "DEFER THE PRODUCT SURFACES",
            rect(20.0, body_y + body_h - 55.0, left_w - 40.0, 16.0),
            10.0,
            620,
            state.selected_probe.accent(),
            7,
        );

        panel(
            &mut nodes,
            "scaffold.proof",
            "Retained renderer diagnostic",
            rect(center_x, body_y, center_w, body_h),
            2,
            Color::BLACK,
        );
        section_label(
            &mut nodes,
            "scaffold.proof.label",
            "RETAINED RENDER PROBE",
            center_x + 24.0,
            body_y + 20.0,
        );
        text(
            &mut nodes,
            "scaffold.proof.selected",
            state.selected_probe.label(),
            rect(center_x + 24.0, body_y + 48.0, center_w - 48.0, 24.0),
            16.0,
            680,
            state.selected_probe.accent(),
            8,
        );
        text(
            &mut nodes,
            "scaffold.proof.detail",
            state.selected_probe.detail(),
            rect(center_x + 24.0, body_y + 80.0, center_w - 48.0, 18.0),
            9.0,
            540,
            Color::MUTED,
            8,
        );

        let canvas = rect(
            center_x + 24.0,
            body_y + 118.0,
            center_w - 48.0,
            body_h - 178.0,
        );
        nodes.push(Node::new(
            id("scaffold.proof.canvas"),
            canvas,
            3,
            NodeKind::Rect { fill: PLANE_RAISED },
        ));
        nodes.push(Node::new(
            id("scaffold.proof.canvas.border"),
            canvas,
            4,
            NodeKind::Stroke {
                color: Color::SEAM,
                width: 1.0,
            },
        ));
        let columns = ((canvas.width / 52.0).floor() as usize).max(1);
        for column in 1..columns {
            let x = canvas.x + column as f32 * canvas.width / columns as f32;
            seam(
                &mut nodes,
                &format!("scaffold.grid.column.{column}"),
                x,
                canvas.y,
                1.0,
                canvas.height,
                4,
            );
        }
        let rows = ((canvas.height / 52.0).floor() as usize).max(1);
        for row in 1..rows {
            let y = canvas.y + row as f32 * canvas.height / rows as f32;
            nodes.push(Node::new(
                id(&format!("scaffold.grid.row.{row}")),
                rect(canvas.x, y, canvas.width, 1.0),
                4,
                NodeKind::Rect { fill: GRID },
            ));
        }

        let sample_w = (canvas.width * 0.52)
            .clamp(230.0, 360.0)
            .min(canvas.width - 48.0);
        let sample_h = (canvas.height * 0.42)
            .clamp(170.0, 280.0)
            .min(canvas.height - 48.0);
        let sample_x = canvas.x + (canvas.width - sample_w) * 0.5;
        let sample_y = canvas.y + (canvas.height - sample_h) * 0.42;
        let compact_sample = sample_w < 280.0;
        nodes.push(Node::new(
            id("scaffold.sample.plane"),
            rect(sample_x, sample_y, sample_w, sample_h),
            6,
            NodeKind::Rect {
                fill: state.selected_probe.soft(),
            },
        ));
        nodes.push(Node::new(
            id("scaffold.sample.border"),
            rect(sample_x, sample_y, sample_w, sample_h),
            7,
            NodeKind::Stroke {
                color: state.selected_probe.accent(),
                width: 1.0,
            },
        ));
        nodes.push(Node::new(
            id("scaffold.sample.signal"),
            rect(sample_x, sample_y, 3.0, sample_h),
            8,
            NodeKind::Rect {
                fill: state.selected_probe.accent(),
            },
        ));
        nodes.push(Node::new(
            id("scaffold.sample.icon"),
            rect(
                sample_x + 24.0,
                sample_y + 22.0,
                if compact_sample { 34.0 } else { 44.0 },
                if compact_sample { 34.0 } else { 44.0 },
            ),
            9,
            NodeKind::Icon {
                name: state.selected_probe.node_id().to_owned(),
                color: state.selected_probe.accent(),
            },
        ));
        text(
            &mut nodes,
            "scaffold.sample.title",
            state.selected_probe.label(),
            rect(
                sample_x + if compact_sample { 70.0 } else { 84.0 },
                sample_y + 25.0,
                sample_w - if compact_sample { 90.0 } else { 104.0 },
                18.0,
            ),
            if compact_sample { 9.5 } else { 10.5 },
            650,
            Color::WHITE,
            9,
        );
        text(
            &mut nodes,
            "scaffold.sample.metric",
            state.selected_probe.metric(),
            rect(
                sample_x + if compact_sample { 24.0 } else { 84.0 },
                sample_y + if compact_sample { 70.0 } else { 50.0 },
                sample_w - if compact_sample { 48.0 } else { 104.0 },
                16.0,
            ),
            if compact_sample { 8.0 } else { 8.5 },
            530,
            Color::MUTED,
            9,
        );
        text(
            &mut nodes,
            "scaffold.sample.text",
            &state.text_sample,
            rect(
                sample_x + 24.0,
                sample_y + sample_h - 38.0,
                sample_w - 48.0,
                18.0,
            ),
            10.0,
            560,
            Color::WHITE,
            9,
        );

        text(
            &mut nodes,
            "scaffold.proof.footer",
            "PIXELS + HIT TEST + FOCUS + SEMANTICS / ONE RETAINED SCENE",
            rect(
                center_x + 24.0,
                body_y + body_h - 40.0,
                center_w - 48.0,
                16.0,
            ),
            9.0,
            560,
            Color::MUTED,
            8,
        );

        panel(
            &mut nodes,
            "handoff",
            "Checkpoint handoff boundary",
            rect(right_x, body_y, right_w, body_h),
            2,
            PLANE_RAISED,
        );
        seam(&mut nodes, "handoff.seam", right_x, body_y, 1.0, body_h, 4);
        section_label(
            &mut nodes,
            "handoff.label",
            "CHECKPOINT HANDOFF",
            right_x + 22.0,
            body_y + 20.0,
        );
        text(
            &mut nodes,
            "handoff.delivered.label",
            "C001 ESTABLISHES",
            rect(right_x + 22.0, body_y + 58.0, right_w - 44.0, 16.0),
            9.0,
            650,
            Color::GREEN,
            7,
        );
        for (index, label) in [
            "POLICY + 201 CHECKPOINT PROGRAM",
            "CRATE + DEPENDENCY BOUNDARIES",
            "SCENE + TEXT + ICON FOUNDATION",
            "INPUT + FOCUS + ACCESSIBILITY",
            "WGPU HOST + PRIVATE CAPTURE",
        ]
        .into_iter()
        .enumerate()
        {
            handoff_row(
                &mut nodes,
                &format!("handoff.delivered.{index}"),
                label,
                right_x + 22.0,
                body_y + 88.0 + index as f32 * 28.0,
                right_w - 44.0,
                Color::GREEN,
            );
        }
        seam(
            &mut nodes,
            "handoff.divider",
            right_x + 22.0,
            body_y + 244.0,
            right_w - 44.0,
            1.0,
            6,
        );
        text(
            &mut nodes,
            "handoff.deferred.label",
            "LATER CHECKPOINTS BUILD",
            rect(right_x + 22.0, body_y + 270.0, right_w - 44.0, 16.0),
            9.0,
            650,
            Color::AMBER,
            7,
        );
        for (index, label) in [
            "SHELL + DOCKING + WINDOWS",
            "MEDIA + SOURCE + VIEWERS",
            "TIMELINE + EDITING + INSPECTOR",
            "COMPOSITE + COLOR + AUDIO",
            "DELIVERY + AUTOMATION + POLISH",
        ]
        .into_iter()
        .enumerate()
        {
            handoff_row(
                &mut nodes,
                &format!("handoff.deferred.{index}"),
                label,
                right_x + 22.0,
                body_y + 302.0 + index as f32 * 28.0,
                right_w - 44.0,
                Color::AMBER,
            );
        }
        text(
            &mut nodes,
            "handoff.guardrail",
            "NO LATER SURFACE IS COMPOSED HERE",
            rect(right_x + 22.0, body_y + body_h - 54.0, right_w - 44.0, 16.0),
            9.0,
            650,
            state.selected_probe.accent(),
            7,
        );

        panel(
            &mut nodes,
            "status",
            "Scaffold status",
            rect(3.0, height - status_h, width - 3.0, status_h),
            3,
            PLANE_RAISED,
        );
        seam(
            &mut nodes,
            "status.seam",
            3.0,
            height - status_h,
            width - 3.0,
            1.0,
            5,
        );
        text(
            &mut nodes,
            "status.foundation",
            "SCAFFOLD READY",
            rect(18.0, height - 22.0, 126.0, 14.0),
            9.0,
            650,
            Color::GREEN,
            7,
        );
        text(
            &mut nodes,
            "status.scope",
            "PRODUCT SURFACES DEFERRED",
            rect(160.0, height - 22.0, 196.0, 14.0),
            9.0,
            560,
            Color::AMBER,
            7,
        );
        text(
            &mut nodes,
            "status.program",
            "201 CHECKPOINTS / PRIVATE DETERMINISTIC PROOF",
            rect(width - 348.0, height - 22.0, 330.0, 14.0),
            9.0,
            520,
            Color::MUTED,
            7,
        );

        Scene::new(
            self.logical_width,
            self.logical_height,
            self.scale_factor,
            nodes,
            state.focused.clone(),
        )
    }
}

fn panel(nodes: &mut Vec<Node>, identity: &str, name: &str, bounds: Rect, z: i32, fill: Color) {
    nodes.push(
        Node::new(id(identity), bounds, z, NodeKind::Rect { fill })
            .with_semantics(SemanticSpec::new(SemanticRole::Group, name)),
    );
}

fn seam(nodes: &mut Vec<Node>, identity: &str, x: f32, y: f32, width: f32, height: f32, z: i32) {
    nodes.push(Node::new(
        id(identity),
        rect(x, y, width, height),
        z,
        NodeKind::Rect { fill: Color::SEAM },
    ));
}

#[allow(clippy::too_many_arguments)]
fn text(
    nodes: &mut Vec<Node>,
    identity: &str,
    content: &str,
    bounds: Rect,
    size: f32,
    weight: u16,
    color: Color,
    z: i32,
) {
    nodes.push(Node::new(
        id(identity),
        bounds,
        z,
        NodeKind::Text {
            text: content.to_owned(),
            size,
            weight,
            color,
        },
    ));
}

fn section_label(nodes: &mut Vec<Node>, identity: &str, content: &str, x: f32, y: f32) {
    text(
        nodes,
        identity,
        content,
        rect(x, y, 210.0, 15.0),
        9.5,
        650,
        Color::MUTED,
        7,
    );
}

fn foundation_probe(nodes: &mut Vec<Node>, probe: FoundationProbe, bounds: Rect, selected: bool) {
    let fill = if selected {
        PLANE_SELECTED
    } else {
        PLANE_ACTIVE
    };
    nodes.push(
        Node::new(id(probe.node_id()), bounds, 8, NodeKind::Rect { fill })
            .with_semantics(
                SemanticSpec::new(SemanticRole::Button, probe.accessible_name())
                    .with_description(probe.detail())
                    .selected(selected)
                    .activatable(),
            )
            .focusable(),
    );
    if selected {
        nodes.push(Node::new(
            id(&format!("{}.signal", probe.node_id())),
            rect(bounds.x, bounds.y + 7.0, 3.0, bounds.height - 14.0),
            10,
            NodeKind::Rect {
                fill: probe.accent(),
            },
        ));
    }
    nodes.push(Node::new(
        id(&format!("{}.icon", probe.node_id())),
        rect(bounds.x + 13.0, bounds.y + 11.0, 32.0, 32.0),
        10,
        NodeKind::Icon {
            name: probe.node_id().to_owned(),
            color: if selected {
                probe.accent()
            } else {
                Color::MUTED
            },
        },
    ));
    text(
        nodes,
        &format!("{}.label", probe.node_id()),
        probe.label(),
        rect(bounds.x + 56.0, bounds.y + 13.0, bounds.width - 68.0, 16.0),
        9.5,
        if selected { 650 } else { 560 },
        if selected { Color::WHITE } else { Color::MUTED },
        10,
    );
    text(
        nodes,
        &format!("{}.state", probe.node_id()),
        if selected { "SELECTED" } else { "READY" },
        rect(bounds.x + 56.0, bounds.y + 33.0, bounds.width - 68.0, 13.0),
        8.0,
        540,
        if selected {
            probe.accent()
        } else {
            Color::MUTED
        },
        10,
    );
}

fn handoff_row(
    nodes: &mut Vec<Node>,
    identity: &str,
    label: &str,
    x: f32,
    y: f32,
    width: f32,
    signal: Color,
) {
    nodes.push(Node::new(
        id(&format!("{identity}.signal")),
        rect(x, y + 4.0, 3.0, 7.0),
        7,
        NodeKind::Rect { fill: signal },
    ));
    text(
        nodes,
        &format!("{identity}.text"),
        label,
        rect(x + 14.0, y, width - 14.0, 15.0),
        8.7,
        540,
        Color::WHITE,
        7,
    );
}

fn id(value: &str) -> NodeId {
    NodeId::new(value).expect("fixture identities are statically valid")
}

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect::new(x, y, width, height).expect("fixture bounds are statically valid")
}
