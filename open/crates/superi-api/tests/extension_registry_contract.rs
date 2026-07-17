use std::sync::Arc;

use serde_json::json;
use superi_api::commands::ApiCommand;
use superi_api::editor::{
    EditorExtensionLifecycle, EditorExtensionMutation, EditorExtensionRecord,
    ExecuteProjectCommand, ProjectAction, ProjectCommand, ProjectEditorApi,
};
use superi_api::event_stream::{PublicApiEvent, PublicEventCorrelation};
use superi_api::events::{ApiEvent, ExtensionsChanged};
use superi_api::extensions::{
    ExtensionIdentity, ExtensionLifecycle, ExtensionRegistryApi, ExtensionRegistrySnapshot,
    GetExtensions,
};
use superi_api::permissions::{
    ApiPermissionContext, ApiPermissionEffect, ApiPermissionRequirementMode,
    ApiPermissionRequirements, ApiPermissionRule, ApiPluginOperation, ApiPluginScope,
};
use superi_api::schema::{ApiResource, PublicMethodKind};
use superi_api::version::{
    EXTENSIONS_CHANGED_EVENT, EXTENSIONS_RESOURCE, EXTENSIONS_SCHEMA_VERSION, GET_EXTENSIONS_METHOD,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::ids::{ProjectId, TimelineId};
use superi_core::settings::{
    CapabilityId, CapabilitySet, ComponentId, FeatureAvailability, FeatureDescriptor, FeatureId,
    SemanticVersion, VersionIdentifier,
};
use superi_core::time::{RationalTime, Timebase};
use superi_engine::editor as engine;
use superi_engine::extensions::{
    ExtensionLifecycle as EngineLifecycle, ExtensionRegistration as EngineRegistration,
    ExtensionRegistry as EngineRegistry,
};

fn capability(value: &str) -> CapabilityId {
    CapabilityId::new(value).unwrap()
}

fn registration(lifecycle: EngineLifecycle) -> EngineRegistration {
    let granted = CapabilitySet::new([capability("superi.capability.project-read")]);
    EngineRegistration::versioned(
        VersionIdentifier::new(
            ComponentId::new("example.discoverable-extension").unwrap(),
            SemanticVersion::new(2, 3, 4),
        ),
        "Discoverable Extension",
        CapabilitySet::new([
            capability("superi.capability.project-read"),
            capability("superi.capability.project-mutate"),
        ]),
        granted.clone(),
        lifecycle,
        [FeatureDescriptor::new(
            FeatureId::new("example.discoverable-extension.render").unwrap(),
            SemanticVersion::new(1, 0, 0),
            if lifecycle == EngineLifecycle::Ready {
                FeatureAvailability::Available
            } else {
                FeatureAvailability::Disabled
            },
            granted,
        )],
        None,
    )
    .unwrap()
}

#[test]
fn query_is_strict_permission_free_and_exposes_only_declarative_control() {
    let mut engine = EngineRegistry::new();
    engine
        .register(registration(EngineLifecycle::Ready))
        .unwrap();
    let api = ExtensionRegistryApi::new(&engine.snapshot());
    let result = api.execute(GetExtensions::new());
    let snapshot = result.snapshot();

    assert_eq!(GetExtensions::METHOD, GET_EXTENSIONS_METHOD);
    assert_eq!(GetExtensions::KIND, PublicMethodKind::Query);
    assert_eq!(GetExtensions::SCHEMA_VERSION, EXTENSIONS_SCHEMA_VERSION);
    assert_eq!(
        GetExtensions::PERMISSION_MODE,
        ApiPermissionRequirementMode::None
    );
    assert_eq!(
        GetExtensions::new().permission_requirements().unwrap(),
        ApiPermissionRequirements::none()
    );
    assert_eq!(ExtensionRegistrySnapshot::RESOURCE, EXTENSIONS_RESOURCE);
    assert_eq!(snapshot.registrations().len(), 1);

    let extension = &snapshot.registrations()[0];
    assert_eq!(extension.lifecycle(), ExtensionLifecycle::Ready);
    assert!(matches!(
        extension.identity(),
        ExtensionIdentity::Versioned { producer }
            if producer == "example.discoverable-extension@2.3.4"
    ));
    assert_eq!(
        extension.granted_capabilities(),
        ["superi.capability.project-read"]
    );
    assert_eq!(
        extension.discovery().capabilities(),
        extension.granted_capabilities()
    );
    assert_eq!(extension.discovery().features().len(), 1);
    assert_eq!(
        extension.control().command_method(),
        "superi.project.command.execute"
    );
    assert_eq!(extension.control().state_resource(), "superi.editor.state");
    assert_eq!(extension.control().operations().len(), 6);

    let encoded = serde_json::to_value(snapshot).unwrap();
    let decoded: ExtensionRegistrySnapshot = serde_json::from_value(encoded.clone()).unwrap();
    assert_eq!(&decoded, snapshot);
    let mut native = encoded["registrations"][0].clone();
    native["identity"] = json!({
        "kind": "native_audio",
        "provider": "superi.audio-plugin-host@1.0.0",
        "format": "vst3",
        "vendor": "Example Vendor",
        "identifier": "EXAMPLE-COMPONENT",
        "version": "2.4.1"
    });
    native["requested_capabilities"] = json!([]);
    native["granted_capabilities"] = json!([]);
    native["discovery"]["producer"] = json!("superi.audio-plugin-host@1.0.0");
    native["discovery"]["capabilities"] = json!([]);
    native["discovery"]["features"] = json!([]);
    native["control"]["extension_id"] = json!("superi.audio-plugin-host");
    let mut mixed = encoded.clone();
    mixed["registrations"] = json!([encoded["registrations"][0].clone(), native]);
    assert!(serde_json::from_value::<ExtensionRegistrySnapshot>(mixed.clone()).is_ok());
    mixed["registrations"].as_array_mut().unwrap().reverse();
    assert!(serde_json::from_value::<ExtensionRegistrySnapshot>(mixed).is_err());
    let serialized = serde_json::to_string(snapshot).unwrap();
    for forbidden in [
        "dispatcher",
        "worker_adapter",
        "bundle_path",
        "candidate_source",
        "permission_context",
    ] {
        assert!(!serialized.contains(forbidden));
    }

    let mut unknown = encoded.clone();
    unknown["registrations"][0]["guessed"] = json!(true);
    assert!(serde_json::from_value::<ExtensionRegistrySnapshot>(unknown).is_err());
    let mut backdoor = encoded;
    backdoor["registrations"][0]["control"]["command_method"] =
        json!("superi.engine.private.dispatch");
    assert!(serde_json::from_value::<ExtensionRegistrySnapshot>(backdoor).is_err());
}

#[test]
fn semantic_changes_emit_one_revisioned_full_replacement_event() {
    let mut engine = EngineRegistry::new();
    engine
        .register(registration(EngineLifecycle::Ready))
        .unwrap();
    let mut api = ExtensionRegistryApi::new(&engine.snapshot());
    assert!(api.synchronize(&engine.snapshot()).unwrap().is_none());

    engine
        .synchronize([registration(EngineLifecycle::Disabled)])
        .unwrap();
    let event = api.synchronize(&engine.snapshot()).unwrap().unwrap();
    assert_eq!(event.snapshot().revision(), 2);
    assert_eq!(
        event.snapshot().registrations()[0].lifecycle(),
        ExtensionLifecycle::Disabled
    );
    assert_eq!(ExtensionsChanged::NAME, EXTENSIONS_CHANGED_EVENT);

    let public = PublicApiEvent::try_from(event).unwrap();
    assert_eq!(public.name(), EXTENSIONS_CHANGED_EVENT);
    assert_eq!(
        public.correlation(),
        PublicEventCorrelation::Observation { revision: 2 }
    );
    assert_eq!(
        public.replacement_resource().descriptor().resource(),
        EXTENSIONS_RESOURCE
    );
    assert_eq!(
        public.replacement_resource().descriptor().method(),
        GET_EXTENSIONS_METHOD
    );
    assert_eq!(
        public.replacement_resource().descriptor().method_kind(),
        PublicMethodKind::Query
    );
    assert!(api.synchronize(&engine.snapshot()).unwrap().is_none());
}

#[test]
fn missing_runtime_never_mutates_durable_project_extension_state() {
    let _domain = ExecutionDomain::EngineControl.enter_current().unwrap();
    let timebase = Timebase::integer(24).unwrap();
    let project_id = ProjectId::from_raw(0xc026_0001);
    let timeline_id = TimelineId::from_raw(0xc026_0002);
    let timeline = engine::Timeline::new(
        timeline_id,
        "extension registry project",
        timebase,
        RationalTime::zero(timebase),
        vec![],
    );
    let editorial =
        engine::EditorialProject::new(project_id, "extension registry project", [], [timeline])
            .unwrap();
    let document = engine::ProjectDocument::new(editorial, timeline_id).unwrap();
    let mut dispatcher = engine::EngineCommandDispatcher::new().unwrap();
    dispatcher.attach_project(document).unwrap();
    let permissions = Arc::new(
        ApiPermissionContext::new(
            "superi.test.extension-registry",
            [
                ApiPermissionRule::plugin(
                    ApiPermissionEffect::Allow,
                    ApiPluginOperation::ManageState,
                    ApiPluginScope::exact("example.missing-runtime").unwrap(),
                )
                .unwrap(),
                ApiPermissionRule::plugin(
                    ApiPermissionEffect::Allow,
                    ApiPluginOperation::ManageLifecycle,
                    ApiPluginScope::exact("example.missing-runtime").unwrap(),
                )
                .unwrap(),
            ],
        )
        .unwrap(),
    );
    let mut editor = ProjectEditorApi::new_with_permissions(dispatcher, permissions).unwrap();
    let command = ExecuteProjectCommand::new(
        "extension-registry-upsert",
        0,
        ProjectCommand::Apply {
            actions: vec![ProjectAction::MutateExtension {
                mutation: Box::new(EditorExtensionMutation::Upsert {
                    record: EditorExtensionRecord {
                        extension_id: "example.missing-runtime".to_owned(),
                        record_id: "durable-state".to_owned(),
                        extension_version: SemanticVersion::new(1, 0, 0),
                        extension_kind: "superi.extension.plugin".to_owned(),
                        payload_schema: "example.missing-runtime-state@1.0.0".to_owned(),
                        requested_capabilities: vec![],
                        granted_capabilities: vec![],
                        lifecycle: EditorExtensionLifecycle::Enabled,
                        failure: None,
                        payload: b"durable opaque state".to_vec(),
                    },
                }),
            }],
        },
    );
    let result = editor.execute(command).unwrap();
    assert!(result.authored_state_changed());
    assert_eq!(editor.drain_events().unwrap().len(), 1);

    let before = editor.project_snapshot().unwrap();
    assert_eq!(before.extension_records().len(), 1);
    let empty_runtime = EngineRegistry::new();
    let discovery =
        ExtensionRegistryApi::new(&empty_runtime.snapshot()).execute(GetExtensions::new());
    assert!(discovery.snapshot().registrations().is_empty());
    let after = editor.project_snapshot().unwrap();
    assert_eq!(after, before);
    assert_eq!(
        after.extension_records().values().next().unwrap().payload(),
        b"durable opaque state"
    );

    let path = std::env::temp_dir().join(format!(
        "superi-extension-registry-{}-{}.superi",
        std::process::id(),
        project_id.raw()
    ));
    {
        let mut database = engine::ProjectDatabase::create(&path).unwrap();
        database.replace(&after).unwrap();
    }
    let reopened = engine::ProjectDatabase::open_read_only(&path).unwrap();
    let loaded = reopened.load().unwrap().snapshot();
    assert_eq!(loaded.extension_records().len(), 1);
    assert_eq!(
        loaded
            .extension_records()
            .values()
            .next()
            .unwrap()
            .payload(),
        b"durable opaque state"
    );
    drop(reopened);
    let _ = std::fs::remove_file(path);
}
