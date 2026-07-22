use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use superi_desktop::desktop_shell::{
    DesktopCloseReason, DesktopPanelDockId, DesktopPanelDockPresentation,
    DesktopRoutePanelLayoutPresentation, DesktopShellDocument, DesktopShellIntent,
    DesktopShellModel, DesktopShellState, DesktopShellSync, DesktopWorkspacePresentation,
};

static NEXT_SHELL_ROOT: AtomicU64 = AtomicU64::new(0);

fn temporary_shell_root(label: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let ordinal = NEXT_SHELL_ROOT.fetch_add(1, Ordering::Relaxed);
    std::env::temp_dir().join(format!(
        "superi-desktop-shell-{label}-{}-{suffix}-{ordinal}",
        std::process::id()
    ))
}

fn workspace(route: &str) -> DesktopWorkspacePresentation {
    let workspace_panel = format!("workspace.{route}");
    DesktopWorkspacePresentation {
        active_route_id: route.to_owned(),
        hidden_panel_ids: vec!["application.selection".to_owned()],
        focused_panel_id: Some(workspace_panel.clone()),
        panel_layouts: vec![DesktopRoutePanelLayoutPresentation {
            route_id: route.to_owned(),
            docks: vec![
                DesktopPanelDockPresentation {
                    dock_id: DesktopPanelDockId::Left,
                    panel_ids: Vec::new(),
                    active_panel_id: None,
                    size_basis_points: 2_400,
                },
                DesktopPanelDockPresentation {
                    dock_id: DesktopPanelDockId::Center,
                    panel_ids: vec![workspace_panel.clone()],
                    active_panel_id: Some(workspace_panel),
                    size_basis_points: 10_000,
                },
                DesktopPanelDockPresentation {
                    dock_id: DesktopPanelDockId::Right,
                    panel_ids: vec!["application.selection".to_owned()],
                    active_panel_id: None,
                    size_basis_points: 2_800,
                },
                DesktopPanelDockPresentation {
                    dock_id: DesktopPanelDockId::Bottom,
                    panel_ids: Vec::new(),
                    active_panel_id: None,
                    size_basis_points: 3_000,
                },
            ],
        }],
    }
}

fn sync(sequence: u64) -> DesktopShellSync {
    DesktopShellSync {
        client_sequence: sequence,
        active: Some(DesktopShellDocument {
            path: "/projects/alpha.superi".to_owned(),
            project_id: "project-alpha".to_owned(),
            project_revision: 17,
        }),
        recent_paths: vec![
            "/projects/alpha.superi".to_owned(),
            "/projects/beta.superi".to_owned(),
        ],
        undo_depth: 4,
        redo_depth: 1,
        busy: false,
        workspace: workspace("editing"),
    }
}

#[test]
fn native_shell_sync_preserves_document_history_workspace_and_recent_intents() {
    let mut model = DesktopShellModel::default();
    let first = model.synchronize(sync(7)).unwrap();
    assert_eq!(first.revision(), 1);
    assert_eq!(first.client_sequence(), 7);
    assert_eq!(first.active().unwrap().project_id(), "project-alpha");
    assert_eq!(first.active().unwrap().project_revision(), 17);
    assert_eq!(first.undo_depth(), 4);
    assert_eq!(first.redo_depth(), 1);
    assert_eq!(first.workspace().active_route_id, "editing");
    assert_eq!(first.workspace().panel_layouts.len(), 1);
    assert_eq!(
        first.workspace().panel_layouts[0].docks[1]
            .active_panel_id
            .as_deref(),
        Some("workspace.editing")
    );
    assert_eq!(first.recent_paths().len(), 2);
    assert_eq!(
        model.intent_for_menu_id("superi.file.recent.1"),
        Some(DesktopShellIntent::OpenRecent {
            path: "/projects/beta.superi".to_owned(),
        })
    );

    let stale = model.synchronize(sync(6)).unwrap();
    assert_eq!(stale, first);
    assert_eq!(model.intent_for_menu_id("superi.file.recent.2"), None);
    assert_eq!(
        model.intent_for_menu_id("superi.workspace.color"),
        Some(DesktopShellIntent::OpenWorkspace {
            route_id: "color".to_owned(),
        })
    );
    assert_eq!(
        model.intent_for_menu_id("superi.edit.command_palette"),
        Some(DesktopShellIntent::OpenCommandPalette)
    );

    let mut long_path = sync(8);
    let path = format!("/projects/{}/alpha.superi", "nested/".repeat(90));
    long_path.active.as_mut().unwrap().path = path.clone();
    long_path.recent_paths = vec![path];
    assert_eq!(model.synchronize(long_path).unwrap().client_sequence(), 8);
}

#[test]
fn close_resolution_is_single_use_and_duplicate_requests_are_suppressed() {
    let mut model = DesktopShellModel::default();
    model.synchronize(sync(1)).unwrap();
    assert_eq!(
        model.request_close(DesktopCloseReason::Window),
        Some(DesktopShellIntent::RequestClose {
            reason: DesktopCloseReason::Window,
        })
    );
    assert_eq!(model.request_close(DesktopCloseReason::Quit), None);
    assert!(!model.resolve_close(false));

    assert_eq!(
        model.request_close(DesktopCloseReason::Quit),
        Some(DesktopShellIntent::RequestClose {
            reason: DesktopCloseReason::Quit,
        })
    );
    assert!(model.resolve_close(true));
    assert!(!model.resolve_close(true));
}

#[test]
fn shell_state_rejects_ambiguous_or_unbounded_presentation() {
    let mut model = DesktopShellModel::default();
    let mut duplicate = sync(1);
    duplicate.recent_paths = vec![
        "/projects/alpha.superi".to_owned(),
        "/projects/alpha.superi".to_owned(),
    ];
    assert!(model.synchronize(duplicate).is_err());

    let mut invalid_workspace = sync(2);
    invalid_workspace.workspace.active_route_id = " ".to_owned();
    assert!(model.synchronize(invalid_workspace).is_err());
    assert_eq!(model.snapshot().revision(), 0);
}

#[test]
fn workspace_presentation_restores_from_the_private_atomic_session_record() {
    let root = temporary_shell_root("restore");

    let first = DesktopShellState::default();
    first.initialize(&root).unwrap();
    let mut presentation = sync(1);
    presentation.workspace = workspace("color");
    first.synchronize(presentation).unwrap();
    let stored: serde_json::Value =
        serde_json::from_slice(&std::fs::read(root.join("desktop-shell-state.json")).unwrap())
            .unwrap();
    assert_eq!(stored["schema_version"].as_u64(), Some(2));
    drop(first);

    let restored = DesktopShellState::default();
    restored.initialize(&root).unwrap();
    let snapshot = restored.snapshot().unwrap();
    assert_eq!(snapshot.workspace().active_route_id, "color");
    assert_eq!(
        snapshot.workspace().hidden_panel_ids,
        ["application.selection"]
    );
    assert_eq!(
        snapshot.workspace().focused_panel_id.as_deref(),
        Some("workspace.color")
    );
    assert_eq!(snapshot.workspace().panel_layouts[0].route_id, "color");
    assert_eq!(
        snapshot.workspace().panel_layouts[0].docks[1].size_basis_points,
        10_000
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_persisted_workspace_degrades_to_defaults_without_blocking_startup() {
    let root = temporary_shell_root("invalid");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("desktop-shell-state.json"),
        br#"{
  "schema_version": 1,
  "workspace": {
    "active_route_id": " ",
    "hidden_panel_ids": [],
    "focused_panel_id": null,
    "panel_layouts": []
  }
}"#,
    )
    .unwrap();

    let state = DesktopShellState::default();
    state.initialize(&root).unwrap();
    let snapshot = state.snapshot().unwrap();
    assert_eq!(snapshot.workspace().active_route_id, "editing");
    assert_eq!(
        snapshot.failure().unwrap().code,
        "desktop_shell_storage_invalid"
    );

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn version_one_workspace_records_migrate_without_inventing_registry_layout() {
    let root = temporary_shell_root("version-one");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("desktop-shell-state.json"),
        br#"{
  "schema_version": 1,
  "workspace": {
    "active_route_id": "color",
    "hidden_panel_ids": ["application.selection"],
    "focused_panel_id": "workspace.color"
  }
}"#,
    )
    .unwrap();

    let state = DesktopShellState::default();
    state.initialize(&root).unwrap();
    let snapshot = state.snapshot().unwrap();
    assert_eq!(snapshot.workspace().active_route_id, "color");
    assert!(snapshot.workspace().panel_layouts.is_empty());
    assert!(snapshot.failure().is_none());

    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn invalid_panel_layout_is_rejected_before_live_state_changes() {
    let mut model = DesktopShellModel::default();
    let accepted = model.synchronize(sync(1)).unwrap();
    let mut invalid = sync(2);
    invalid.workspace.panel_layouts[0].docks[0].panel_ids = vec!["workspace.editing".to_owned()];
    assert!(model.synchronize(invalid).is_err());
    assert_eq!(model.snapshot(), &accepted);
}

#[test]
fn workspace_persistence_failure_keeps_live_native_presentation_available() {
    let root = temporary_shell_root("read-only");

    let state = DesktopShellState::default();
    state.initialize(&root).unwrap();
    std::fs::create_dir(root.join("desktop-shell-state.json")).unwrap();
    let mut presentation = sync(1);
    presentation.workspace = workspace("delivery");
    let snapshot = state.synchronize(presentation).unwrap();
    assert_eq!(snapshot.workspace().active_route_id, "delivery");
    assert_eq!(
        snapshot.failure().unwrap().code,
        "desktop_shell_storage_publish_failed"
    );

    std::fs::remove_dir_all(root).unwrap();
}
