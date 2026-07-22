use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use superi_core::error::{ErrorCategory, Recoverability};
use superi_desktop::crash_diagnostics::{
    DesktopCrashDiagnostics, DesktopCrashFailureClass, DesktopCrashRecoveryEntryPoint,
    DesktopPanelDockId, DesktopPanelDockPresentation, DesktopProjectContinuity,
    DesktopRoutePanelLayoutPresentation, DesktopWorkspaceContinuity,
};
use superi_desktop::lifecycle::{ApplicationLifecycle, HeadlessEngineFailure};

#[test]
fn an_unclean_session_retains_workspace_and_project_recovery_intent() {
    let root = test_root("unclean");
    let first = DesktopCrashDiagnostics::default();
    let started = first.initialize(root.clone());
    assert!(started.diagnostics().is_empty());

    first
        .update_workspace(
            DesktopWorkspaceContinuity::new(
                "editing",
                ["application.selection"],
                Some("workspace.editing"),
            )
            .unwrap()
            .with_panel_layouts(vec![editing_layout()])
            .unwrap(),
        )
        .unwrap();
    first
        .update_project(Some(
            DesktopProjectContinuity::new("/projects/cut.superi", 42).unwrap(),
        ))
        .unwrap();
    drop(first);

    let second = DesktopCrashDiagnostics::default();
    let recovered = second.initialize(root.clone());
    assert_eq!(recovered.diagnostics().len(), 1);
    let diagnostic = &recovered.diagnostics()[0];
    assert_eq!(diagnostic.code(), "unexpected_exit");
    assert_eq!(
        diagnostic.failure_class(),
        DesktopCrashFailureClass::Retryable
    );
    assert_eq!(
        diagnostic.recovery_entry_point(),
        DesktopCrashRecoveryEntryPoint::RestoreWorkspace
    );
    assert_eq!(diagnostic.continuity().workspace().route_id(), "editing");
    assert_eq!(
        diagnostic.continuity().workspace().hidden_panel_ids(),
        &["application.selection"]
    );
    assert_eq!(
        diagnostic.continuity().workspace().focused_panel_id(),
        Some("workspace.editing")
    );
    assert_eq!(
        diagnostic.continuity().workspace().panel_layouts()[0].docks[1]
            .active_panel_id
            .as_deref(),
        Some("workspace.editing")
    );
    assert_eq!(
        diagnostic
            .continuity()
            .project()
            .expect("project intent should survive")
            .path(),
        "/projects/cut.superi"
    );
    assert_eq!(
        diagnostic
            .continuity()
            .project()
            .unwrap()
            .project_revision(),
        42
    );

    second.finish_session().unwrap();
    remove_test_root(root);
}

#[test]
fn layout_free_version_one_session_marker_remains_recoverable() {
    let root = test_root("legacy-layout");
    std::fs::create_dir_all(&root).unwrap();
    std::fs::write(
        root.join("active-session.json"),
        br#"{
  "schema_version": 1,
  "session_id": "session-legacy",
  "started_unix_millis": 1,
  "updated_unix_millis": 2,
  "continuity": {
    "workspace": {
      "route_id": "editing",
      "hidden_panel_ids": [],
      "focused_panel_id": "workspace.editing"
    },
    "project": null,
    "lifecycle": null
  }
}"#,
    )
    .unwrap();

    let diagnostics = DesktopCrashDiagnostics::default();
    let snapshot = diagnostics.initialize(root.clone());
    let recovered = snapshot
        .diagnostics()
        .iter()
        .find(|diagnostic| diagnostic.code() == "unexpected_exit")
        .unwrap();
    assert!(recovered
        .continuity()
        .workspace()
        .panel_layouts()
        .is_empty());
    diagnostics.finish_session().unwrap();
    remove_test_root(root);
}

#[test]
fn orderly_shutdown_clears_only_the_current_session_marker() {
    let root = test_root("clean");
    let first = DesktopCrashDiagnostics::default();
    let _ = first.initialize(root.clone());
    first.finish_session().unwrap();

    let second = DesktopCrashDiagnostics::default();
    let restarted = second.initialize(root.clone());
    assert!(restarted.diagnostics().is_empty());
    second.finish_session().unwrap();
    remove_test_root(root);
}

#[test]
fn marker_ownership_and_interrupted_publication_preserve_the_newer_session() {
    let root = test_root("marker-owner");
    let first = DesktopCrashDiagnostics::default();
    let _ = first.initialize(root.clone());
    let second = DesktopCrashDiagnostics::default();
    let _ = second.initialize(root.clone());

    let error = first.finish_session().unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert!(root.join("active-session.json").is_file());
    second.finish_session().unwrap();

    let recovered = DesktopCrashDiagnostics::default();
    let before_interruption = recovered.initialize(root.clone());
    let unexpected_before = before_interruption
        .diagnostics()
        .iter()
        .filter(|diagnostic| diagnostic.code() == "unexpected_exit")
        .count();
    std::fs::rename(
        root.join("active-session.json"),
        root.join(".active-session.json.previous"),
    )
    .unwrap();
    drop(recovered);

    let after_interruption = DesktopCrashDiagnostics::default();
    let snapshot = after_interruption.initialize(root.clone());
    assert_eq!(
        snapshot
            .diagnostics()
            .iter()
            .filter(|diagnostic| diagnostic.code() == "unexpected_exit")
            .count(),
        unexpected_before + 1
    );
    after_interruption.finish_session().unwrap();
    remove_test_root(root);
}

#[test]
fn lifecycle_failures_preserve_all_four_recovery_classes_and_entry_points() {
    let cases = [
        (
            Recoverability::Retryable,
            DesktopCrashFailureClass::Retryable,
            DesktopCrashRecoveryEntryPoint::RetryEngine,
        ),
        (
            Recoverability::Degraded,
            DesktopCrashFailureClass::Degraded,
            DesktopCrashRecoveryEntryPoint::ContinueDegraded,
        ),
        (
            Recoverability::UserCorrectable,
            DesktopCrashFailureClass::UserCorrectable,
            DesktopCrashRecoveryEntryPoint::ReviewProjectRecovery,
        ),
        (
            Recoverability::Terminal,
            DesktopCrashFailureClass::Terminal,
            DesktopCrashRecoveryEntryPoint::RestartEngine,
        ),
    ];

    for (index, (recoverability, expected_class, expected_entry)) in cases.into_iter().enumerate() {
        let root = test_root(&format!("class-{index}"));
        let diagnostics = DesktopCrashDiagnostics::default();
        let _ = diagnostics.initialize(root.clone());
        diagnostics
            .update_project(Some(
                DesktopProjectContinuity::new("/projects/cut.superi", 7).unwrap(),
            ))
            .unwrap();

        let lifecycle = ApplicationLifecycle::new().unwrap();
        let engine = lifecycle.headless_engine_participant().unwrap();
        let failed = engine
            .fail(
                HeadlessEngineFailure::new(
                    ErrorCategory::Unavailable,
                    recoverability,
                    "The engine operation needs attention.",
                )
                .with_context("superi-desktop.engine", "connect"),
            )
            .unwrap();
        let observed = diagnostics.observe_lifecycle(&failed).unwrap();
        let diagnostic = observed.diagnostics().last().unwrap();
        assert_eq!(diagnostic.failure_class(), expected_class);
        assert_eq!(diagnostic.recovery_entry_point(), expected_entry);
        assert_eq!(
            diagnostic.contexts()[0].component(),
            "superi-desktop.engine"
        );
        assert_eq!(diagnostic.contexts()[0].operation(), "connect");
        assert_eq!(diagnostic.action().trim(), diagnostic.action());
        assert!(!diagnostic.action().is_empty());

        diagnostics.finish_session().unwrap();
        remove_test_root(root);
    }
}

#[test]
fn panic_detail_stays_private_while_corrupt_storage_degrades_safely() {
    let root = test_root("panic");
    let diagnostics = DesktopCrashDiagnostics::default();
    let _ = diagnostics.initialize(root.clone());
    let snapshot = diagnostics
        .record_panic(
            "private panic payload at /secret/source.rs",
            Some("source.rs:12"),
        )
        .unwrap();
    let serialized = serde_json::to_string(&snapshot).unwrap();
    assert!(!serialized.contains("private panic payload"));
    assert!(!serialized.contains("/secret/source.rs"));
    let journal = std::fs::read_to_string(root.join("diagnostics.json")).unwrap();
    assert!(journal.contains("private panic payload at /secret/source.rs"));
    diagnostics.finish_session().unwrap();

    std::fs::write(root.join("diagnostics.json"), b"not valid json").unwrap();
    let reopened = DesktopCrashDiagnostics::default();
    let degraded = reopened.initialize(root.clone());
    assert!(degraded.diagnostics().iter().any(|diagnostic| {
        diagnostic.failure_class() == DesktopCrashFailureClass::Degraded
            && diagnostic.code() == "diagnostic_store_corrupt"
    }));
    assert!(std::fs::read_dir(&root).unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("diagnostics.json.corrupt-")
    }));
    reopened.finish_session().unwrap();
    remove_test_root(root);
}

fn test_root(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    std::env::temp_dir().join(format!(
        "superi-crash-diagnostics-{label}-{}-{nonce}",
        std::process::id()
    ))
}

fn remove_test_root(root: PathBuf) {
    std::fs::remove_dir_all(root).unwrap();
}

fn editing_layout() -> DesktopRoutePanelLayoutPresentation {
    DesktopRoutePanelLayoutPresentation {
        route_id: "editing".to_owned(),
        docks: vec![
            DesktopPanelDockPresentation {
                dock_id: DesktopPanelDockId::Left,
                panel_ids: Vec::new(),
                active_panel_id: None,
                size_basis_points: 2_400,
            },
            DesktopPanelDockPresentation {
                dock_id: DesktopPanelDockId::Center,
                panel_ids: vec!["workspace.editing".to_owned()],
                active_panel_id: Some("workspace.editing".to_owned()),
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
    }
}
