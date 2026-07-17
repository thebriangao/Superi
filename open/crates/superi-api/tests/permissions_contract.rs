use std::fs;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::json;
use sha2::{Digest, Sha256};
use superi_api::commands::{
    ApiCommand, CancelAsyncJob, ExecuteAudioAutomationTransaction, ExecuteScenarioAction,
};
use superi_api::editor::{ExecuteProjectCommand, ProjectEditorApi};
use superi_api::permissions::{
    ApiDestructiveOperation, ApiFilesystemAccess, ApiFilesystemPath, ApiFilesystemPlatform,
    ApiFilesystemScope, ApiPermissionContext, ApiPermissionEffect, ApiPermissionKind,
    ApiPermissionRequirement, ApiPermissionRequirementMode, ApiPermissionRequirements,
    ApiPermissionRule, ApiPluginOperation, ApiPluginScope,
};
use superi_api::scenario::{ExactFrameRate, ScenarioApi, SliceAction};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::ids::{ProjectId, TimelineId};
use superi_core::time::Timebase;
use superi_engine::dispatcher::EngineCommandDispatcher;
use superi_engine::test_support::empty_project_document;

const PROJECT: ProjectId = ProjectId::from_raw(0xc020);
const ROOT: TimelineId = TimelineId::from_raw(0xc021);

fn context(rules: impl IntoIterator<Item = ApiPermissionRule>) -> ApiPermissionContext {
    ApiPermissionContext::new("superi.host.permissions-test", rules).unwrap()
}

#[test]
fn filesystem_scopes_are_component_aware_platform_typed_and_deny_first() {
    let safe_root =
        ApiFilesystemPath::absolute(ApiFilesystemPlatform::Unix, "/safe/media").unwrap();
    assert_eq!(
        safe_root.as_absolute(),
        Some((ApiFilesystemPlatform::Unix, "/safe/media"))
    );
    assert!(safe_root.as_project_relative().is_none());
    let allowed = ApiPermissionRule::filesystem(
        ApiPermissionEffect::Allow,
        ApiFilesystemAccess::Read,
        ApiFilesystemScope::recursive(safe_root.clone()),
    );
    let denied_file =
        ApiFilesystemPath::absolute(ApiFilesystemPlatform::Unix, "/safe/media/private.mov")
            .unwrap();
    let explicit_deny = ApiPermissionRule::filesystem(
        ApiPermissionEffect::Deny,
        ApiFilesystemAccess::Read,
        ApiFilesystemScope::exact(denied_file.clone()),
    );
    let permissions = context([allowed, explicit_deny]);

    permissions
        .authorize(
            "superi.test.filesystem",
            &ApiPermissionRequirements::new([ApiPermissionRequirement::filesystem(
                ApiFilesystemAccess::Read,
                ApiFilesystemPath::absolute(ApiFilesystemPlatform::Unix, "/safe/media/clip.mov")
                    .unwrap(),
            )])
            .unwrap(),
        )
        .unwrap();

    let denied = permissions
        .authorize(
            "superi.test.filesystem",
            &ApiPermissionRequirements::new([ApiPermissionRequirement::filesystem(
                ApiFilesystemAccess::Read,
                denied_file,
            )])
            .unwrap(),
        )
        .unwrap_err();
    assert_eq!(denied.category(), ErrorCategory::PermissionDenied);
    assert_eq!(denied.recoverability(), Recoverability::UserCorrectable);
    let denied_text = format!("{denied:?}");
    assert!(!denied_text.contains("private.mov"));
    assert!(!denied_text.contains("/safe/media"));

    for outside in [
        ApiFilesystemPath::absolute(ApiFilesystemPlatform::Unix, "/safe/mediator/clip.mov")
            .unwrap(),
        ApiFilesystemPath::absolute(ApiFilesystemPlatform::Unix, "/safe/media/../secret.mov")
            .unwrap(),
        ApiFilesystemPath::absolute(ApiFilesystemPlatform::Windows, "C:\\safe\\media\\clip.mov")
            .unwrap(),
    ] {
        let failure = permissions
            .authorize(
                "superi.test.filesystem",
                &ApiPermissionRequirements::new([ApiPermissionRequirement::filesystem(
                    ApiFilesystemAccess::Read,
                    outside,
                )])
                .unwrap(),
            )
            .unwrap_err();
        assert_eq!(failure.category(), ErrorCategory::PermissionDenied);
    }

    assert!(ApiFilesystemPath::project_relative("../secret.mov").is_err());
    let project_permissions = context([ApiPermissionRule::filesystem(
        ApiPermissionEffect::Allow,
        ApiFilesystemAccess::Write,
        ApiFilesystemScope::recursive(ApiFilesystemPath::project_relative("media/render").unwrap()),
    )]);
    let portable = ApiFilesystemPath::project_relative("media/./render/../source.mov").unwrap();
    assert_eq!(portable.as_project_relative(), Some("media/source.mov"));
    assert!(portable.as_absolute().is_none());
    let sibling = ApiPermissionRequirements::new([ApiPermissionRequirement::filesystem(
        ApiFilesystemAccess::Write,
        ApiFilesystemPath::project_relative("media/rendered/output.mov").unwrap(),
    )])
    .unwrap();
    assert_eq!(
        project_permissions
            .authorize("superi.test.filesystem", &sibling)
            .unwrap_err()
            .category(),
        ErrorCategory::PermissionDenied
    );

    let noncanonical_rule: ApiPermissionRule = serde_json::from_value(json!({
        "kind": "filesystem",
        "effect": "allow",
        "access": "read",
        "scope": {
            "kind": "exact",
            "path": {
                "kind": "absolute",
                "platform": "unix",
                "path": "/safe/media/../escape.mov"
            }
        }
    }))
    .unwrap();
    assert_eq!(
        ApiPermissionContext::new("superi.test.noncanonical-path", [noncanonical_rule])
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
}

#[test]
fn plugin_scopes_and_delegation_ceilings_are_exact_and_deny_first() {
    let allow_all_state = ApiPermissionRule::plugin(
        ApiPermissionEffect::Allow,
        ApiPluginOperation::ManageState,
        ApiPluginScope::all(),
    )
    .unwrap();
    let deny_alpha_state = ApiPermissionRule::plugin(
        ApiPermissionEffect::Deny,
        ApiPluginOperation::ManageState,
        ApiPluginScope::exact("example.alpha").unwrap(),
    )
    .unwrap();
    let delegate_alpha = ApiPermissionRule::plugin_delegation(
        ApiPermissionEffect::Allow,
        ApiPluginScope::exact("example.alpha").unwrap(),
        ["project.read", "render.submit"],
    )
    .unwrap();
    let permissions = context([allow_all_state, deny_alpha_state, delegate_alpha]);

    let alpha_state = ApiPermissionRequirements::new([ApiPermissionRequirement::plugin(
        ApiPluginOperation::ManageState,
        "example.alpha",
    )
    .unwrap()])
    .unwrap();
    assert_eq!(
        permissions
            .authorize("superi.test.plugin", &alpha_state)
            .unwrap_err()
            .category(),
        ErrorCategory::PermissionDenied
    );

    let beta_state = ApiPermissionRequirements::new([ApiPermissionRequirement::plugin(
        ApiPluginOperation::ManageState,
        "example.beta",
    )
    .unwrap()])
    .unwrap();
    permissions
        .authorize("superi.test.plugin", &beta_state)
        .unwrap();

    let allowed_delegation =
        ApiPermissionRequirements::new([ApiPermissionRequirement::plugin_delegation(
            "example.alpha",
            ["project.read"],
        )
        .unwrap()])
        .unwrap();
    permissions
        .authorize("superi.test.plugin", &allowed_delegation)
        .unwrap();

    for denied in [
        ApiPermissionRequirement::plugin_delegation("example.alpha", ["network.open"]).unwrap(),
        ApiPermissionRequirement::plugin_delegation("example.alphabet", ["project.read"]).unwrap(),
    ] {
        let failure = permissions
            .authorize(
                "superi.test.plugin",
                &ApiPermissionRequirements::new([denied]).unwrap(),
            )
            .unwrap_err();
        assert_eq!(failure.category(), ErrorCategory::PermissionDenied);
    }

    let noncanonical_rule: ApiPermissionRule = serde_json::from_value(json!({
        "kind": "plugin",
        "effect": "allow",
        "operation": "delegate_capabilities",
        "scope": {"kind": "exact", "extension_id": "example.alpha"},
        "delegation_ceiling": ["render.submit", "project.read"]
    }))
    .unwrap();
    assert_eq!(
        ApiPermissionContext::new("superi.test.noncanonical-delegation", [noncanonical_rule])
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
}

#[test]
fn destructive_command_requirements_are_explicit_and_operation_scoped() {
    assert_eq!(
        CancelAsyncJob::PERMISSION_MODE,
        ApiPermissionRequirementMode::Static
    );
    assert_eq!(
        CancelAsyncJob::PERMISSION_KINDS,
        &[ApiPermissionKind::Destructive]
    );
    let cancel = CancelAsyncJob::new(
        "cancel",
        superi_api::jobs::AsyncJobHandle::new("job:00000000000000000000000000000001").unwrap(),
    );
    let requirements = cancel.permission_requirements().unwrap();
    assert_eq!(
        requirements.as_slice(),
        &[ApiPermissionRequirement::destructive(
            ApiDestructiveOperation::CancelAsyncJob
        )]
    );

    let permissions = context([ApiPermissionRule::destructive(
        ApiPermissionEffect::Allow,
        ApiDestructiveOperation::CancelAsyncJob,
    )]);
    permissions
        .authorize(CancelAsyncJob::METHOD, &requirements)
        .unwrap();
    assert_eq!(
        permissions
            .authorize(
                "superi.jobs.remove",
                &ApiPermissionRequirements::new([ApiPermissionRequirement::destructive(
                    ApiDestructiveOperation::RemoveAsyncJob,
                ),])
                .unwrap(),
            )
            .unwrap_err()
            .category(),
        ErrorCategory::PermissionDenied
    );

    let ordinary = ExecuteAudioAutomationTransaction::new(
        "ordinary",
        0,
        vec![
            superi_api::audio_automation::AudioAutomationMutation::SetMode {
                target: superi_api::audio_automation::AudioAutomationTarget::ClipGain {
                    clip_id: "clip:00000000000000000000000000000001".to_owned(),
                },
                mode: superi_api::audio_automation::AudioAutomationMode::Read,
            },
        ],
    );
    assert!(ordinary.permission_requirements().unwrap().is_empty());
    let destructive = ExecuteAudioAutomationTransaction::new(
        "destructive",
        0,
        vec![
            superi_api::audio_automation::AudioAutomationMutation::RemoveLane {
                target: superi_api::audio_automation::AudioAutomationTarget::ClipGain {
                    clip_id: "clip:00000000000000000000000000000001".to_owned(),
                },
            },
        ],
    );
    assert_eq!(
        destructive.permission_requirements().unwrap().as_slice(),
        &[ApiPermissionRequirement::destructive(
            ApiDestructiveOperation::RemoveAudioAutomation
        )]
    );
}

#[test]
fn denied_plugin_mutation_preserves_project_state_sequence_and_events() {
    let _domain = ExecutionDomain::EngineControl.enter_current().unwrap();
    let mut dispatcher = EngineCommandDispatcher::new().unwrap();
    dispatcher
        .attach_project(
            empty_project_document(PROJECT, ROOT, Timebase::integer(24).unwrap()).unwrap(),
        )
        .unwrap();
    let mut denied_api = ProjectEditorApi::new(dispatcher).unwrap();
    let before = denied_api.project_snapshot().unwrap();
    let command = extension_upsert("denied-extension");
    let requirements = command.permission_requirements().unwrap();
    assert_eq!(requirements.len(), 3);
    let failure = denied_api.execute(command).unwrap_err();
    assert_eq!(failure.category(), ErrorCategory::PermissionDenied);
    assert_eq!(denied_api.project_snapshot().unwrap(), before);
    assert!(denied_api.drain_events().unwrap().is_empty());

    let mut dispatcher = EngineCommandDispatcher::new().unwrap();
    dispatcher
        .attach_project(
            empty_project_document(PROJECT, ROOT, Timebase::integer(24).unwrap()).unwrap(),
        )
        .unwrap();
    let permissions = Arc::new(context([
        ApiPermissionRule::plugin(
            ApiPermissionEffect::Allow,
            ApiPluginOperation::ManageState,
            ApiPluginScope::exact("example.permissioned").unwrap(),
        )
        .unwrap(),
        ApiPermissionRule::plugin(
            ApiPermissionEffect::Allow,
            ApiPluginOperation::ManageLifecycle,
            ApiPluginScope::exact("example.permissioned").unwrap(),
        )
        .unwrap(),
        ApiPermissionRule::plugin_delegation(
            ApiPermissionEffect::Allow,
            ApiPluginScope::exact("example.permissioned").unwrap(),
            ["project.read"],
        )
        .unwrap(),
    ]));
    let mut allowed_api = ProjectEditorApi::new_with_permissions(dispatcher, permissions).unwrap();
    let result = allowed_api
        .execute(extension_upsert("allowed-extension"))
        .unwrap();
    assert!(result.authored_state_changed());
    assert_eq!(result.command_sequence(), 1);
    assert_eq!(
        allowed_api
            .project_snapshot()
            .unwrap()
            .extension_records()
            .len(),
        1
    );
    assert_eq!(allowed_api.drain_events().unwrap().len(), 1);
}

#[test]
fn denied_filesystem_action_preserves_scenario_state_and_exact_allow_restores_parity() {
    let directory = std::env::temp_dir().join(format!(
        "superi-api-permissions-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    fs::create_dir_all(&directory).unwrap();
    let source = directory.join("input.webm");
    fs::write(&source, b"permission fixture").unwrap();
    let action = import_action(&source);

    let mut denied_api = ScenarioApi::new();
    let before = denied_api.snapshot();
    let failure = denied_api
        .execute(ExecuteScenarioAction::new(action.clone()))
        .unwrap_err();
    assert_eq!(failure.category(), "permission_denied");
    assert_eq!(failure.recoverability(), "user_correctable");
    assert_eq!(denied_api.snapshot(), before);
    assert!(denied_api.drain_events().is_empty());
    assert!(!serde_json::to_string(&failure)
        .unwrap()
        .contains(source.to_str().unwrap()));

    let exact_path = ApiFilesystemPath::native(source.to_str().unwrap()).unwrap();
    let permissions = Arc::new(context([ApiPermissionRule::filesystem(
        ApiPermissionEffect::Allow,
        ApiFilesystemAccess::Read,
        ApiFilesystemScope::exact(exact_path),
    )]));
    let mut allowed_api = ScenarioApi::new_with_permissions(permissions);
    let result = allowed_api
        .execute(ExecuteScenarioAction::new(action))
        .unwrap();
    assert_eq!(result.state().revision(), 1);
    assert_eq!(allowed_api.drain_events().len(), 1);

    fs::remove_dir_all(directory).unwrap();
}

fn extension_upsert(transaction_id: &str) -> ExecuteProjectCommand {
    serde_json::from_value(json!({
        "transaction_id": transaction_id,
        "expected_project_revision": 0,
        "command": {
            "command": "apply",
            "actions": [{
                "action": "mutate_extension",
                "mutation": {
                    "operation": "upsert",
                    "record": {
                        "extension_id": "example.permissioned",
                        "record_id": "state",
                        "extension_version": "1.0.0",
                        "extension_kind": "superi.extension.state",
                        "payload_schema": "example.permissioned-state@1.0.0",
                        "requested_capabilities": ["project.read"],
                        "granted_capabilities": ["project.read"],
                        "lifecycle": "enabled",
                        "failure": null,
                        "payload": [112, 117, 98, 108, 105, 99]
                    }
                }
            }]
        }
    }))
    .unwrap()
}

fn import_action(path: &std::path::Path) -> SliceAction {
    SliceAction::ImportClip {
        path: path.display().to_string(),
        fixture_id: "slice/video-cfr".to_owned(),
        fixture_version: 1,
        manifest_sha256: "1d2b28b5f44c7f86dce50d67b718b0fad967d267d9016961e3d71bb9dab94419"
            .to_owned(),
        payload_sha256: format!("{:x}", Sha256::digest(fs::read(path).unwrap())),
        frame_rate: ExactFrameRate::new(24, 1),
        frame_count: 96,
        width: 96,
        height: 54,
    }
}
