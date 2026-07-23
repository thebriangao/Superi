use accesskit::{Action as AccessAction, Role as AccessRole};
use superi_ui::fixture::{FoundationFixture, FoundationState};
use superi_ui::icons::IconRegistry;
use superi_ui::input::{InputEvent, InteractionController, Key};
use superi_ui::paint::CpuPainter;
use superi_ui::scene::NodeId;
use superi_ui::semantics::SemanticRole;

#[test]
fn retained_scene_drives_pixels_hits_focus_and_semantics() {
    let fixture = FoundationFixture::new(1440, 900, 1.0).expect("valid fixture");
    let state = FoundationState::default();
    let scene = fixture.scene(&state).expect("foundation scene");
    let node = NodeId::new("foundation.semantics").expect("stable node");

    assert_eq!(scene.hit_test_node(&node), Some(node.clone()));
    let semantics = scene.semantics();
    let semantic = semantics.node(&node).expect("probe has semantic output");
    assert_eq!(semantic.role(), SemanticRole::Button);
    assert_eq!(semantic.name(), "Semantic projection probe");
    assert!(semantic.actions().activate());
    let accesskit = semantics
        .accesskit_update()
        .expect("native semantic update");
    let (_, semantic_accessibility) = accesskit
        .nodes
        .iter()
        .find(|(_, node)| node.label() == Some("Semantic projection probe"))
        .expect("probe in native semantic tree");
    assert_eq!(semantic_accessibility.role(), AccessRole::Button);
    assert!(semantic_accessibility.supports_action(AccessAction::Click));
    assert!(accesskit.tree.is_some());

    let first = CpuPainter::new().paint(&scene).expect("first raster");
    let second = CpuPainter::new().paint(&scene).expect("second raster");
    assert_eq!(first, second);
}

#[test]
fn interaction_changes_pixels_and_semantics_through_the_same_scene() {
    let fixture = FoundationFixture::new(1440, 900, 1.0).expect("valid fixture");
    let mut controller = InteractionController::new(FoundationState::default());
    let before = fixture.scene(controller.state()).expect("before scene");
    let before_pixels = CpuPainter::new().paint(&before).expect("before pixels");

    controller
        .dispatch(
            &before,
            InputEvent::Activate(NodeId::new("foundation.semantics").expect("stable node")),
        )
        .expect("activate semantic probe");
    let after = fixture.scene(controller.state()).expect("after scene");
    let after_pixels = CpuPainter::new().paint(&after).expect("after pixels");
    let after_semantics = after.semantics();
    let probe_id = NodeId::new("foundation.semantics").expect("stable node");
    let probe = after_semantics.node(&probe_id).expect("probe semantics");

    assert_ne!(before_pixels, after_pixels);
    assert!(probe.selected());
    assert_eq!(after.focused(), Some(probe.id()));
    let transcript = serde_json::to_value(controller.transcript()).expect("serialize transcript");
    assert_eq!(transcript[0]["event"]["kind"], "activate");
    assert_eq!(transcript[0]["event"]["value"], "foundation.semantics");

    controller
        .dispatch(&after, InputEvent::Key(Key::Tab))
        .expect("tab traversal");
    assert_ne!(controller.state().focused(), Some(probe.id()));
}

#[test]
fn icon_registry_is_original_versioned_and_collision_free() {
    let registry = IconRegistry::foundation();
    registry.validate().expect("valid registry");
    assert!(registry.get("foundation.scene").is_some());
    assert!(registry.get("foundation.input").is_some());
    assert!(registry.get("foundation.semantics").is_some());
    assert!(registry.get("foundation.capture").is_some());
    assert_eq!(registry.iter().len(), 4);
    assert_eq!(registry.duplicates().len(), 0);
}

#[test]
fn scaffold_scene_contains_no_later_product_surface() {
    let fixture = FoundationFixture::new(1440, 900, 1.0).expect("valid fixture");
    let scene = fixture
        .scene(&FoundationState::default())
        .expect("foundation scene");
    let forbidden = [
        "workspace.",
        "tool.",
        "media.",
        "viewer",
        "transport.",
        "timeline",
        "inspector",
    ];
    for node in scene.nodes() {
        assert!(
            !forbidden
                .iter()
                .any(|prefix| node.id().as_str().starts_with(prefix)),
            "scaffold contains later product node `{}`",
            node.id()
        );
    }
}
