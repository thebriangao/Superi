use std::str::FromStr;

use serde_json::json;
use superi_api::commands::ApiCommand;
use superi_api::negotiation::{
    ApiVersionIncompatibility, NegotiateApiVersion, ProjectCompatibilityDisposition,
    ProjectCompatibilityReason, ProjectFormatDescriptor, VersionNegotiationApi,
};
use superi_api::permissions::{ApiPermissionRequirementMode, ApiPermissionRequirements};
use superi_api::schema::{JsonRpcRequest, JsonRpcSuccessResponse, PublicMethodKind};
use superi_core::settings::SemanticVersion;

fn version(value: &str) -> SemanticVersion {
    SemanticVersion::from_str(value).unwrap()
}

#[test]
fn negotiation_selects_highest_common_canonical_versions_and_projects_migrations() {
    let request = NegotiateApiVersion::try_new(
        vec![version("1.0.0"), version("1.1.0+client")],
        vec![1],
        Some(ProjectFormatDescriptor::new(
            0x5355_5052,
            "superi.project",
            version("1.0.0"),
            1,
            1,
        )),
    )
    .unwrap();

    assert_eq!(NegotiateApiVersion::METHOD, "superi.api.version.negotiate");
    assert_eq!(NegotiateApiVersion::KIND, PublicMethodKind::Query);
    assert_eq!(NegotiateApiVersion::SCHEMA_VERSION, version("1.0.0"));
    assert_eq!(
        NegotiateApiVersion::PERMISSION_MODE,
        ApiPermissionRequirementMode::None
    );
    assert_eq!(
        request.permission_requirements().unwrap(),
        ApiPermissionRequirements::none()
    );

    let result = VersionNegotiationApi::new().execute(request);
    assert_eq!(
        result
            .support()
            .api_schema_versions()
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>(),
        vec!["1.0.0", "1.1.0", "1.2.0", "1.3.0", "1.4.0"]
    );
    assert_eq!(result.support().primitive_schema_revisions(), &[1]);
    let selection = result.selection().unwrap();
    assert_eq!(selection.api_schema_version(), &version("1.1.0"));
    assert_eq!(selection.primitive_schema_revision(), 1);
    assert!(result.incompatibilities().is_empty());

    let project = result.project().unwrap();
    assert_eq!(
        project.disposition(),
        ProjectCompatibilityDisposition::MigrationRequired
    );
    assert_eq!(
        project.reasons(),
        &[ProjectCompatibilityReason::RegisteredMigration]
    );
    assert_eq!(
        project
            .migration_path()
            .iter()
            .map(|release| release.schema_revision())
            .collect::<Vec<_>>(),
        vec![2, 3, 4]
    );
}

#[test]
fn negotiation_reports_each_missing_dimension_and_evaluates_project_independently() {
    let request = NegotiateApiVersion::try_new(
        vec![version("9.0.0")],
        vec![2],
        Some(ProjectFormatDescriptor::new(
            0x5355_5052,
            "superi.project",
            version("2.0.0"),
            2,
            5,
        )),
    )
    .unwrap();

    let result = VersionNegotiationApi::new().execute(request);
    assert!(result.selection().is_none());
    assert_eq!(
        result.incompatibilities(),
        &[
            ApiVersionIncompatibility::NoCommonApiSchemaVersion,
            ApiVersionIncompatibility::NoCommonPrimitiveSchemaRevision,
        ]
    );
    let project = result.project().unwrap();
    assert_eq!(
        project.disposition(),
        ProjectCompatibilityDisposition::RequiresNewerApplication
    );
    assert_eq!(
        project.reasons(),
        &[
            ProjectCompatibilityReason::FutureSchemaRevision,
            ProjectCompatibilityReason::FutureSemanticFormat,
            ProjectCompatibilityReason::FuturePrimitiveRevision,
        ]
    );
}

#[test]
fn client_offers_are_strict_bounded_and_ordered_by_semver_precedence() {
    for value in [
        json!({"api_schema_versions": [], "primitive_schema_revisions": [1]}),
        json!({"api_schema_versions": ["1.1.0", "1.0.0"], "primitive_schema_revisions": [1]}),
        json!({"api_schema_versions": ["1.1.0+one", "1.1.0+two"], "primitive_schema_revisions": [1]}),
        json!({"api_schema_versions": ["1.0.0"], "primitive_schema_revisions": []}),
        json!({"api_schema_versions": ["1.0.0"], "primitive_schema_revisions": [1, 1]}),
        json!({"api_schema_versions": ["1.0.0"], "primitive_schema_revisions": [0]}),
        json!({"api_schema_versions": ["1.0.0"], "primitive_schema_revisions": [1], "extra": true}),
    ] {
        assert!(serde_json::from_value::<NegotiateApiVersion>(value).is_err());
    }

    assert!(NegotiateApiVersion::try_new(
        vec![version("1.1.0-alpha.1"), version("1.1.0")],
        vec![1],
        None,
    )
    .is_ok());
}

#[test]
fn negotiation_round_trips_through_strict_json_rpc_two_point_zero() {
    let request = NegotiateApiVersion::try_new(vec![version("1.2.0")], vec![1], None).unwrap();
    let rpc = JsonRpcRequest::new("negotiate-1", request).unwrap();
    let encoded = serde_json::to_value(&rpc).unwrap();
    assert_eq!(encoded["jsonrpc"], "2.0");
    assert_eq!(encoded["method"], "superi.api.version.negotiate");
    let decoded: JsonRpcRequest<NegotiateApiVersion> = serde_json::from_value(encoded).unwrap();
    let result = VersionNegotiationApi::new().execute(decoded.into_params());
    let response = JsonRpcSuccessResponse::new("negotiate-1", result).unwrap();
    let response_value = serde_json::to_value(&response).unwrap();
    assert_eq!(response_value["jsonrpc"], "2.0");
    assert_eq!(
        response_value["result"]["selection"]["api_schema_version"],
        "1.2.0"
    );

    let mut invalid = serde_json::to_value(rpc).unwrap();
    invalid["jsonrpc"] = json!("1.0");
    assert!(serde_json::from_value::<JsonRpcRequest<NegotiateApiVersion>>(invalid).is_err());
}
