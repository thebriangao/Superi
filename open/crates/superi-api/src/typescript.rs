//! Deterministic TypeScript bindings for the public API surface.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::fmt::Write as _;

use specta::datatype::{DefOpts, NamedDataType, TypeDefs};
use specta::ts::{export_datatype, BigIntExportBehavior, ExportConfiguration};
use specta::NamedType;

use crate::commands::ApiCommand;
use crate::events::ApiEvent;
use crate::schema::{ApiResource, GetPublicApiSchema, PublicApiSchemaApi};

/// Generator-only shadow for the core semantic version string wire.
#[allow(dead_code)]
#[derive(specta::Type)]
#[specta(rename = "SemanticVersion")]
pub(crate) struct SemanticVersionBinding(String);

/// Generator-only shadow for the core error category string wire.
#[allow(dead_code)]
#[derive(specta::Type)]
#[specta(rename = "ErrorCategory", rename_all = "snake_case")]
pub(crate) enum ErrorCategoryBinding {
    InvalidInput,
    NotFound,
    Conflict,
    Unsupported,
    PermissionDenied,
    ResourceExhausted,
    Unavailable,
    Timeout,
    Cancelled,
    CorruptData,
    Internal,
}

/// Generator-only shadow for the core recoverability string wire.
#[allow(dead_code)]
#[derive(specta::Type)]
#[specta(rename = "Recoverability", rename_all = "snake_case")]
pub(crate) enum RecoverabilityBinding {
    Retryable,
    Degraded,
    UserCorrectable,
    Terminal,
}

/// Generator-only shadow for the core feature availability string wire.
#[allow(dead_code)]
#[derive(specta::Type)]
#[specta(rename = "FeatureAvailability", rename_all = "snake_case")]
pub(crate) enum FeatureAvailabilityBinding {
    Available,
    Disabled,
    Unsupported,
    Unavailable,
}

/// Generator-only shadow for the core diagnostic value wire.
#[allow(dead_code)]
#[derive(specta::Type)]
#[specta(rename = "TraceValue", tag = "kind")]
pub(crate) enum TraceValueBinding {
    #[specta(rename = "bool")]
    Boolean { value: bool },
    #[specta(rename = "i64")]
    Signed { value: String },
    #[specta(rename = "u64")]
    Unsigned { value: String },
    #[specta(rename = "f64")]
    Float { value: f64 },
    #[specta(rename = "text")]
    Text { value: String },
}

/// Generator-only shadow for sorted diagnostic fields.
#[allow(dead_code)]
#[derive(specta::Type)]
#[specta(rename = "TraceValueMap")]
pub(crate) struct TraceValueMapBinding(BTreeMap<String, TraceValueBinding>);

/// Generator-only shadow whose emitted definition is replaced by recursive JSON.
#[allow(dead_code)]
#[derive(specta::Type)]
#[specta(rename = "CanonicalJson")]
pub(crate) struct CanonicalJsonBinding(serde_json::Value);

/// Failure while deriving the committed TypeScript contract.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TypeScriptBindingError {
    message: String,
}

impl TypeScriptBindingError {
    fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for TypeScriptBindingError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for TypeScriptBindingError {}

/// One deterministic collection of API types and stable public names.
pub(crate) struct BindingRegistry {
    type_definitions: TypeDefs,
    roots: Vec<NamedDataType>,
    methods: BTreeMap<&'static str, (String, String)>,
    events: BTreeMap<&'static str, String>,
    resources: BTreeMap<&'static str, String>,
}

impl BindingRegistry {
    fn new() -> Self {
        Self {
            type_definitions: TypeDefs::new(),
            roots: Vec::new(),
            methods: BTreeMap::new(),
            events: BTreeMap::new(),
            resources: BTreeMap::new(),
        }
    }

    fn add_named<T: NamedType>(&mut self) -> Result<String, TypeScriptBindingError> {
        let definition = T::definition_named_data_type(DefOpts {
            parent_inline: false,
            type_map: &mut self.type_definitions,
        })
        .map_err(|error| TypeScriptBindingError::new(error.to_string()))?;
        let name = definition.name.to_owned();
        self.roots.push(definition);
        Ok(name)
    }

    /// Adds one catalog-adjacent common type, such as the structured error payload.
    pub(crate) fn register_common<T: NamedType>(&mut self) -> Result<(), TypeScriptBindingError> {
        self.add_named::<T>()?;
        Ok(())
    }

    fn validate_catalog(&self) -> Result<(), TypeScriptBindingError> {
        let catalog = PublicApiSchemaApi::new()
            .map_err(|error| TypeScriptBindingError::new(error.to_string()))?
            .execute(GetPublicApiSchema::new())
            .into_snapshot();

        let catalog_methods = catalog
            .commands()
            .iter()
            .chain(catalog.queries())
            .map(|method| method.method())
            .collect::<BTreeSet<_>>();
        let registered_methods = self.methods.keys().copied().collect::<BTreeSet<_>>();
        if catalog_methods != registered_methods || catalog_methods.len() != self.methods.len() {
            return Err(TypeScriptBindingError::new(
                "TypeScript method registry does not match the canonical public catalog",
            ));
        }

        let catalog_events = catalog
            .events()
            .iter()
            .map(|event| event.event())
            .collect::<BTreeSet<_>>();
        let registered_events = self.events.keys().copied().collect::<BTreeSet<_>>();
        if catalog_events != registered_events || catalog_events.len() != self.events.len() {
            return Err(TypeScriptBindingError::new(
                "TypeScript event registry does not match the canonical public catalog",
            ));
        }

        let catalog_resources = catalog
            .resources()
            .iter()
            .map(|resource| resource.resource())
            .collect::<BTreeSet<_>>();
        let registered_resources = self.resources.keys().copied().collect::<BTreeSet<_>>();
        if catalog_resources != registered_resources
            || catalog_resources.len() != self.resources.len()
        {
            return Err(TypeScriptBindingError::new(
                "TypeScript resource registry does not match the canonical public catalog",
            ));
        }

        Ok(())
    }

    fn named_definitions(self) -> Result<BTreeMap<String, NamedDataType>, TypeScriptBindingError> {
        let mut definitions = BTreeMap::new();
        for definition in self.roots.into_iter().chain(
            self.type_definitions
                .into_values()
                .map(|definition| {
                    definition.ok_or_else(|| {
                        TypeScriptBindingError::new(
                            "Specta left an unresolved recursive TypeScript definition",
                        )
                    })
                })
                .collect::<Result<Vec<_>, _>>()?,
        ) {
            let name = definition.name.to_owned();
            if let Some(existing) = definitions.get(&name) {
                if existing != &definition {
                    return Err(TypeScriptBindingError::new(format!(
                        "multiple Rust types resolve to the TypeScript name {name}"
                    )));
                }
                continue;
            }
            definitions.insert(name, definition);
        }
        Ok(definitions)
    }
}

/// Adds one request and response pair from the canonical command registry.
pub(crate) fn register_method<C>(
    registry: &mut BindingRegistry,
) -> Result<(), TypeScriptBindingError>
where
    C: ApiCommand + NamedType,
    C::Response: NamedType,
{
    let request = registry.add_named::<C>()?;
    let response = registry.add_named::<C::Response>()?;
    if registry
        .methods
        .insert(C::METHOD, (request, response))
        .is_some()
    {
        return Err(TypeScriptBindingError::new(format!(
            "duplicate TypeScript method registration for {}",
            C::METHOD
        )));
    }
    Ok(())
}

/// Adds one payload from the canonical event registry.
pub(crate) fn register_event<E>(
    registry: &mut BindingRegistry,
) -> Result<(), TypeScriptBindingError>
where
    E: ApiEvent + NamedType,
{
    let payload = registry.add_named::<E>()?;
    if registry.events.insert(E::NAME, payload).is_some() {
        return Err(TypeScriptBindingError::new(format!(
            "duplicate TypeScript event registration for {}",
            E::NAME
        )));
    }
    Ok(())
}

/// Adds one replacement payload from the canonical resource registry.
pub(crate) fn register_resource<R>(
    registry: &mut BindingRegistry,
) -> Result<(), TypeScriptBindingError>
where
    R: ApiResource + NamedType,
{
    let payload = registry.add_named::<R>()?;
    if registry.resources.insert(R::RESOURCE, payload).is_some() {
        return Err(TypeScriptBindingError::new(format!(
            "duplicate TypeScript resource registration for {}",
            R::RESOURCE
        )));
    }
    Ok(())
}

/// Renders the complete public API contract without reading or writing the filesystem.
pub fn render_typescript_bindings() -> Result<String, TypeScriptBindingError> {
    let mut registry = BindingRegistry::new();
    crate::schema::register_typescript_surface(&mut registry)?;
    registry.validate_catalog()?;

    let methods = registry.methods.clone();
    let events = registry.events.clone();
    let resources = registry.resources.clone();
    let definitions = registry.named_definitions()?;
    let configuration = ExportConfiguration::new().bigint(BigIntExportBehavior::Number);

    let mut output = String::from(
        "// Generated by superi-api-bindings. Do not edit by hand.\n\n\
         export type CanonicalJson = null | boolean | number | string | CanonicalJson[] | { [key: string]: CanonicalJson };\n\n",
    );
    for (name, definition) in definitions {
        if name == "CanonicalJson" {
            continue;
        }
        let rendered = export_datatype(&configuration, &definition)
            .map_err(|error| TypeScriptBindingError::new(error.to_string()))?;
        output.push_str(&rendered);
        output.push_str(";\n\n");
    }

    output.push_str("export interface SuperiMethodMap {\n");
    for (method, (request, response)) in methods {
        writeln!(
            output,
            "  \"{method}\": {{ request: {request}; response: {response} }};"
        )
        .expect("writing to a String cannot fail");
    }
    output.push_str("}\n\nexport interface SuperiEventMap {\n");
    for (event, payload) in events {
        writeln!(output, "  \"{event}\": {payload};").expect("writing to a String cannot fail");
    }
    output.push_str("}\n\nexport interface SuperiResourceMap {\n");
    for (resource, payload) in resources {
        writeln!(output, "  \"{resource}\": {payload};").expect("writing to a String cannot fail");
    }
    output.push_str(
        "}\n\n\
         export interface SuperiTransport {\n\
           request<M extends keyof SuperiMethodMap>(\n\
             method: M,\n\
             request: SuperiMethodMap[M][\"request\"],\n\
           ): Promise<SuperiMethodMap[M][\"response\"]>;\n\
           subscribe?<E extends keyof SuperiEventMap>(\n\
             event: E,\n\
             listener: (payload: SuperiEventMap[E]) => void,\n\
           ): () => void;\n\
         }\n\n\
         export class SuperiTransportError extends Error {\n\
           public readonly publicError: PublicApiError;\n\n\
           public constructor(publicError: PublicApiError) {\n\
             super(publicError.message);\n\
             this.name = \"SuperiTransportError\";\n\
             this.publicError = publicError;\n\
           }\n\
         }\n\n\
         export class SuperiClient {\n\
           public constructor(private readonly transport: SuperiTransport) {}\n\n\
           public request<M extends keyof SuperiMethodMap>(\n\
             method: M,\n\
             request: SuperiMethodMap[M][\"request\"],\n\
           ): Promise<SuperiMethodMap[M][\"response\"]> {\n\
             return this.transport.request(method, request);\n\
           }\n\n\
           public subscribe<E extends keyof SuperiEventMap>(\n\
             event: E,\n\
             listener: (payload: SuperiEventMap[E]) => void,\n\
           ): () => void {\n\
             if (!this.transport.subscribe) {\n\
               throw new Error(\"the configured Superi transport does not support subscriptions\");\n\
             }\n\
             return this.transport.subscribe(event, listener);\n\
           }\n\
         }\n\n\
         export type JsonRpcId = string;\n\n\
         export interface SuperiJsonRpcRequest<M extends keyof SuperiMethodMap> {\n\
           jsonrpc: \"2.0\";\n\
           id: JsonRpcId;\n\
           method: M;\n\
           params: SuperiMethodMap[M][\"request\"];\n\
         }\n\n\
         export interface SuperiJsonRpcSuccess<M extends keyof SuperiMethodMap> {\n\
           jsonrpc: \"2.0\";\n\
           id: JsonRpcId;\n\
           result: SuperiMethodMap[M][\"response\"];\n\
         }\n\n\
         export interface SuperiJsonRpcFailure {\n\
           jsonrpc: \"2.0\";\n\
           id: JsonRpcId;\n\
           error: PublicApiError;\n\
         }\n\n\
         export type SuperiJsonRpcResponse<M extends keyof SuperiMethodMap> =\n\
           | SuperiJsonRpcSuccess<M>\n\
           | SuperiJsonRpcFailure;\n",
    );

    if output.contains(['\u{2013}', '\u{2014}']) {
        return Err(TypeScriptBindingError::new(
            "generated TypeScript contains a forbidden Unicode dash",
        ));
    }
    Ok(output)
}
