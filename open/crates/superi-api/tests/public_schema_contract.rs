use serde_json::{json, Value};
use superi_api::commands::ApiCommand;
use superi_api::schema::{
    GetPublicApiSchema, GetPublicApiSchemaResult, JsonRpcFailureResponse, JsonRpcRequest,
    JsonRpcResponse, JsonRpcSuccessResponse, PublicApiError, PublicApiSchemaApi,
    PublicApiSchemaSnapshot, PublicErrorContext, PublicMethodKind, PublicResourceReference,
};
use superi_api::version::{
    GET_PUBLIC_API_SCHEMA_METHOD, PUBLIC_API_SCHEMA_VERSION, PUBLIC_ERROR_SCHEMA_VERSION,
};
use superi_core::diagnostics::{DiagnosticEvent, TraceField, TraceValue};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability};
use superi_core::serialization::STABLE_PRIMITIVE_SCHEMA_REVISION;
use superi_core::settings::SemanticVersion;

const COMMANDS: &[&str] = &[
    "superi.audio.automation.transaction.execute",
    "superi.events.subscription.close",
    "superi.events.subscription.open",
    "superi.jobs.cancel",
    "superi.jobs.cancel_all",
    "superi.jobs.pause",
    "superi.jobs.remove",
    "superi.jobs.resume",
    "superi.jobs.retry",
    "superi.project.command.execute",
    "superi.project.recovery.dismiss",
    "superi.project.recovery.restore",
    "superi.project.settings.transaction.execute",
    "superi.slice.scenario.action.execute",
    "superi.slice.scenario.transaction.execute",
];

const QUERIES: &[&str] = &[
    "superi.api.schema.get",
    "superi.audio.automation.get",
    "superi.editor.state.get",
    "superi.engine.integration.validation.get",
    "superi.engine.introspection.get",
    "superi.events.subscription.poll",
    "superi.jobs.get",
    "superi.media.capabilities.get",
    "superi.project.recovery.compare",
    "superi.project.recovery.get",
    "superi.project.settings.get",
];

const EVENTS: &[&str] = &[
    "superi.audio.automation.changed",
    "superi.engine.introspection.changed",
    "superi.jobs.changed",
    "superi.media.capabilities.changed",
    "superi.project.recovery.changed",
    "superi.project.settings.changed",
    "superi.project.state.changed",
    "superi.slice.scenario.state.changed",
];

const RESOURCES: &[&str] = &[
    "superi.audio.automation",
    "superi.editor.state",
    "superi.engine.integration.validation",
    "superi.engine.introspection",
    "superi.events.subscription",
    "superi.jobs",
    "superi.media.capabilities",
    "superi.project.history",
    "superi.project.recovery",
    "superi.project.settings",
    "superi.slice.scenario.state",
];

#[test]
fn current_catalog_is_complete_versioned_sorted_and_deterministic() {
    let api = PublicApiSchemaApi::new().unwrap();
    let first = api.execute(GetPublicApiSchema::new());
    let second = api.execute(GetPublicApiSchema::new());
    assert_eq!(first, second);

    let snapshot = first.snapshot();
    assert_eq!(snapshot.schema_version(), &PUBLIC_API_SCHEMA_VERSION);
    assert_eq!(
        snapshot.primitive_schema_revision(),
        STABLE_PRIMITIVE_SCHEMA_REVISION
    );
    assert_eq!(snapshot.json_rpc_version(), "2.0");
    assert_eq!(names(snapshot.commands(), |value| value.method()), COMMANDS);
    assert_eq!(names(snapshot.queries(), |value| value.method()), QUERIES);
    assert_eq!(names(snapshot.events(), |value| value.event()), EVENTS);
    assert_eq!(
        names(snapshot.resources(), |value| value.resource()),
        RESOURCES
    );
    assert_eq!(
        snapshot.error().schema().version(),
        &PUBLIC_ERROR_SCHEMA_VERSION
    );
    assert_eq!(snapshot.error().categories().len(), 11);
    assert_eq!(snapshot.error().recoverabilities().len(), 4);
    assert_eq!(snapshot.capability().availability().len(), 4);

    assert!(snapshot
        .commands()
        .iter()
        .chain(snapshot.queries())
        .all(|method| {
            let expected = expected_domain_version(method.method());
            method.request().version() == &expected
                && method.response().version() == &expected
                && method.request().primitive_schema_revision() == STABLE_PRIMITIVE_SCHEMA_REVISION
                && method.response().primitive_schema_revision() == STABLE_PRIMITIVE_SCHEMA_REVISION
        }));
    assert!(snapshot.events().iter().all(|event| {
        event.payload().version() == &expected_domain_version(event.event())
            && event.payload().primitive_schema_revision() == STABLE_PRIMITIVE_SCHEMA_REVISION
    }));
    assert!(snapshot.resources().iter().all(|resource| {
        resource.payload().version() == &expected_domain_version(resource.resource())
            && resource.payload().primitive_schema_revision() == STABLE_PRIMITIVE_SCHEMA_REVISION
    }));

    let encoded = serde_json::to_vec(snapshot).unwrap();
    let decoded: PublicApiSchemaSnapshot = serde_json::from_slice(&encoded).unwrap();
    assert_eq!(&decoded, snapshot);
    let mut unknown: Value = serde_json::from_slice(&encoded).unwrap();
    unknown["guessed"] = json!(true);
    assert!(serde_json::from_value::<PublicApiSchemaSnapshot>(unknown).is_err());
}

#[test]
fn catalog_constructor_rejects_duplicates_overlap_and_incompatible_identity() {
    let current = PublicApiSchemaApi::new()
        .unwrap()
        .execute(GetPublicApiSchema::new())
        .into_snapshot();
    let duplicate = current.commands()[0].clone();
    assert!(PublicApiSchemaSnapshot::try_new(
        PUBLIC_API_SCHEMA_VERSION.clone(),
        STABLE_PRIMITIVE_SCHEMA_REVISION,
        "2.0",
        vec![duplicate.clone(), duplicate],
        current.queries().to_vec(),
        current.events().to_vec(),
        current.resources().to_vec(),
        current.error().clone(),
        current.capability().clone(),
    )
    .is_err());

    let overlap = current.commands()[0].clone();
    assert!(PublicApiSchemaSnapshot::try_new(
        PUBLIC_API_SCHEMA_VERSION.clone(),
        STABLE_PRIMITIVE_SCHEMA_REVISION,
        "2.0",
        current.commands().to_vec(),
        current.queries().iter().cloned().chain([overlap]).collect(),
        current.events().to_vec(),
        current.resources().to_vec(),
        current.error().clone(),
        current.capability().clone(),
    )
    .is_err());

    assert!(PublicApiSchemaSnapshot::try_new(
        SemanticVersion::new(2, 0, 0),
        STABLE_PRIMITIVE_SCHEMA_REVISION,
        "2.0",
        current.commands().to_vec(),
        current.queries().to_vec(),
        current.events().to_vec(),
        current.resources().to_vec(),
        current.error().clone(),
        current.capability().clone(),
    )
    .is_err());
    assert!(PublicApiSchemaSnapshot::try_new(
        PUBLIC_API_SCHEMA_VERSION.clone(),
        STABLE_PRIMITIVE_SCHEMA_REVISION + 1,
        "2.0",
        current.commands().to_vec(),
        current.queries().to_vec(),
        current.events().to_vec(),
        current.resources().to_vec(),
        current.error().clone(),
        current.capability().clone(),
    )
    .is_err());
}

#[test]
fn json_rpc_contracts_are_strict_typed_and_mutually_exclusive() {
    assert_eq!(GetPublicApiSchema::METHOD, GET_PUBLIC_API_SCHEMA_METHOD);
    assert_eq!(GetPublicApiSchema::KIND, PublicMethodKind::Query);
    assert_eq!(
        GetPublicApiSchema::SCHEMA_VERSION,
        PUBLIC_API_SCHEMA_VERSION
    );

    let api = PublicApiSchemaApi::new().unwrap();
    let request = JsonRpcRequest::new("schema-request-1", GetPublicApiSchema::new()).unwrap();
    let request_value = serde_json::to_value(&request).unwrap();
    assert_eq!(request_value["jsonrpc"], "2.0");
    assert_eq!(request_value["id"], "schema-request-1");
    assert_eq!(request_value["method"], GET_PUBLIC_API_SCHEMA_METHOD);
    assert_eq!(request_value["params"], json!({}));
    let decoded: JsonRpcRequest<GetPublicApiSchema> =
        serde_json::from_value(request_value.clone()).unwrap();
    assert_eq!(decoded, request);

    let mut wrong_version = request_value.clone();
    wrong_version["jsonrpc"] = json!("1.0");
    assert!(serde_json::from_value::<JsonRpcRequest<GetPublicApiSchema>>(wrong_version).is_err());
    let mut wrong_method = request_value.clone();
    wrong_method["method"] = json!("superi.api.schema.guessed");
    assert!(serde_json::from_value::<JsonRpcRequest<GetPublicApiSchema>>(wrong_method).is_err());
    let mut unknown = request_value;
    unknown["guessed"] = json!(true);
    assert!(serde_json::from_value::<JsonRpcRequest<GetPublicApiSchema>>(unknown).is_err());

    let result = api.execute(decoded.into_params());
    let success = JsonRpcSuccessResponse::new("schema-request-1", result).unwrap();
    let success_value = serde_json::to_value(&success).unwrap();
    let response: JsonRpcResponse<GetPublicApiSchemaResult> =
        serde_json::from_value(success_value.clone()).unwrap();
    assert!(matches!(response, JsonRpcResponse::Success(_)));

    let context = PublicErrorContext::reviewed("superi.api", GET_PUBLIC_API_SCHEMA_METHOD).unwrap();
    let source = Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        "private discovery failure",
    );
    let public_error = PublicApiError::from_error(&source, vec![context], None).unwrap();
    let failure = JsonRpcFailureResponse::new("schema-request-1", public_error).unwrap();
    let failure_value = serde_json::to_value(&failure).unwrap();
    let response: JsonRpcResponse<Value> = serde_json::from_value(failure_value).unwrap();
    assert!(matches!(response, JsonRpcResponse::Failure(_)));

    let mut both = success_value;
    both["error"] = serde_json::to_value(failure.error()).unwrap();
    assert!(serde_json::from_value::<JsonRpcResponse<Value>>(both).is_err());
}

#[test]
fn public_errors_cover_every_recovery_class_and_filter_diagnostics() {
    for recoverability in Recoverability::ALL {
        let source = Error::new(
            ErrorCategory::Conflict,
            *recoverability,
            "raw internal summary",
        )
        .with_context(
            ErrorContext::new("superi.internal", "mutate").with_field("secret", "never publish"),
        );
        let context = PublicErrorContext::reviewed("superi.api", "superi.api.execute").unwrap();
        let public = PublicApiError::from_error(&source, vec![context], None).unwrap();
        assert_eq!(public.data().category(), ErrorCategory::Conflict);
        assert_eq!(public.data().recoverability(), *recoverability);
        assert_eq!(public.data().contexts().len(), 1);
        let encoded = serde_json::to_string(&public).unwrap();
        assert!(!encoded.contains("raw internal summary"));
        assert!(!encoded.contains("never publish"));
    }

    let source = Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Degraded,
        "private /Users/example/project.superi token=secret",
    );
    let diagnostic =
        DiagnosticEvent::from_error("superi.api.discovery_failed", "superi.api", &source)
            .unwrap()
            .with_field("safe_reason", TraceField::user_safe("offline"))
            .unwrap()
            .with_field(
                "internal_path",
                TraceField::internal("/Users/example/project.superi"),
            )
            .unwrap()
            .with_field("sensitive_token", TraceField::sensitive("secret"))
            .unwrap();
    let last_valid = PublicResourceReference::new(
        "superi.project.recovery",
        SemanticVersion::new(1, 0, 0),
        "project-7",
        42,
    )
    .unwrap();
    let public = PublicApiError::from_diagnostic(&diagnostic, Some(last_valid.clone())).unwrap();
    assert_eq!(public.data().category(), ErrorCategory::Unavailable);
    assert_eq!(public.data().recoverability(), Recoverability::Degraded);
    assert_eq!(
        public.data().contexts()[0].fields().get("safe_reason"),
        Some(&TraceValue::from("offline"))
    );
    assert_eq!(public.data().last_valid_resource(), Some(&last_valid));
    assert_eq!(public.data().last_valid_resource().unwrap().revision(), 42);
    let encoded = serde_json::to_string(&public).unwrap();
    assert!(encoded.contains("offline"));
    assert!(!encoded.contains("project.superi"));
    assert!(!encoded.contains("sensitive_token"));
    assert!(!encoded.contains("internal_path"));

    let decoded: PublicApiError = serde_json::from_str(&encoded).unwrap();
    assert_eq!(decoded, public);
    let mut unknown = serde_json::to_value(public).unwrap();
    unknown["data"]["guessed"] = json!(true);
    assert!(serde_json::from_value::<PublicApiError>(unknown).is_err());
}

fn names<T>(values: &[T], name: impl Fn(&T) -> &str) -> Vec<&str> {
    values.iter().map(name).collect()
}

fn expected_domain_version(name: &str) -> SemanticVersion {
    if name == "superi.api.schema.get" {
        PUBLIC_API_SCHEMA_VERSION
    } else if name.starts_with("superi.media.capabilities") {
        SemanticVersion::new(2, 0, 0)
    } else {
        SemanticVersion::new(1, 0, 0)
    }
}
