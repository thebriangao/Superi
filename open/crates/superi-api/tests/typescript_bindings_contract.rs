#![cfg(feature = "typescript-bindings")]

use superi_api::schema::{GetPublicApiSchema, PublicApiSchemaApi};
use superi_api::typescript::render_typescript_bindings;

#[test]
fn generated_typescript_covers_the_canonical_public_catalog_deterministically() {
    let first = render_typescript_bindings().expect("the public API bindings must render");
    let second = render_typescript_bindings().expect("a repeated render must succeed");
    assert_eq!(first, second);

    let catalog = PublicApiSchemaApi::new().expect("the canonical public catalog must validate");
    let snapshot = catalog.execute(GetPublicApiSchema::new()).into_snapshot();
    for method in snapshot.commands().iter().chain(snapshot.queries()) {
        assert!(
            first.contains(method.method()),
            "generated bindings omitted method {}",
            method.method()
        );
    }
    for event in snapshot.events() {
        assert!(
            first.contains(event.event()),
            "generated bindings omitted event {}",
            event.event()
        );
    }
    for resource in snapshot.resources() {
        assert!(
            first.contains(resource.resource()),
            "generated bindings omitted resource {}",
            resource.resource()
        );
    }

    for declaration in [
        "export type ExecuteProjectCommand",
        "export type ExecuteProjectCommandResult",
        "export type ProjectScriptProgram",
        "export type RunProjectScript",
        "export type RunProjectScriptResult",
        "export type ProjectStateChanged",
        "export type EditorAiState",
        "export type GetExtensions",
        "export type ExtensionRegistrySnapshot",
        "export type ExtensionsChanged",
        "export type PublicApiError",
        "export interface SuperiMethodMap",
        "export interface SuperiEventMap",
        "export interface SuperiResourceMap",
        "export class SuperiClient",
    ] {
        assert!(
            first.contains(declaration),
            "generated bindings omitted {declaration}"
        );
    }

    assert!(!first.contains(env!("CARGO_MANIFEST_DIR")));
    assert!(!first.contains("generated_at"));
}
