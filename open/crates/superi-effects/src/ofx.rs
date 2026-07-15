//! OpenFX 1.5.1 compatible image-effect hosting through an isolated adapter.
//!
//! This module owns the safe effect-side protocol. It never loads a native library or calls an
//! OpenFX entry point in the editor process. An [`IsolatedOfxAdapter`] implementation must place
//! native plugin code in a supervised worker process and enforce its declared message bound,
//! render deadline, and restart behavior. The later engine plugin supervisor owns that transport.
//!
//! Scanned clips and parameters become ordinary graph-native [`EffectNodeDefinition`] values.
//! Parameter values remain host-owned editable state, and create or render requests sample one
//! immutable graph revision through an explicit timeline projection before graph links and
//! expressions resolve. Disabled, faulted, and quarantined plugins remain discoverable for editing
//! while their schemas are absent from the active runtime catalog.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::panic::{catch_unwind, AssertUnwindSafe};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::settings::{CapabilityId, CapabilitySet, ComponentId, SemanticVersion};
use superi_graph::expr::ParameterAddress;
use superi_graph::ids::{GraphId, NodeId};
use superi_graph::mutate::{GraphSnapshot, TypedParameterValue};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchemaId, NodeTypeId,
    ParameterName, ParameterSchema, PortCardinality, PortName, PortSchema, RoiBehavior,
    TimeBehavior, ValueTypeId,
};
use superi_graph::value::{FiniteF64, GraphValue};

use crate::authoring::{
    EffectCatalog, EffectMetadata, EffectNodeDefinition, EffectParameterDefinition,
    EffectPortDefinition, ParameterControl,
};

const COMPONENT: &str = "superi-effects.ofx";
const ADAPTER_PROTOCOL_REVISION: u32 = 1;
const MAX_NAME_BYTES: usize = 256;
const MAX_TEXT_BYTES: usize = 4096;
const MAX_RESOURCE_TOKEN_BYTES: usize = 1024;
const MAX_CLIPS_PER_CONTEXT: usize = 64;
const MAX_PARAMETERS_PER_CONTEXT: usize = 4096;
const MAX_CHOICES_PER_PARAMETER: usize = 4096;
const IMAGE_VALUE_TYPE: &str = "superi.value.image";
const OFX_CAPABILITY: &str = "superi.extension.ofx";
const WORKER_CAPABILITY: &str = "superi.extension.native-worker";

/// One host-to-plugin action in the supported OpenFX lifecycle.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum OfxAction {
    /// Loads the plugin in its isolated worker.
    Load,
    /// Describes plugin-wide properties and supported contexts.
    Describe,
    /// Describes clips and parameters for one context.
    DescribeInContext,
    /// Creates one live image-effect instance.
    CreateInstance,
    /// Renders one output region at one time.
    Render,
    /// Destroys one live image-effect instance.
    DestroyInstance,
    /// Unloads the plugin after all instances are destroyed.
    Unload,
    /// Restarts the isolated worker after a failure.
    RestartWorker,
}

impl OfxAction {
    /// Returns the stable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Load => "load",
            Self::Describe => "describe",
            Self::DescribeInContext => "describe_in_context",
            Self::CreateInstance => "create_instance",
            Self::Render => "render",
            Self::DestroyInstance => "destroy_instance",
            Self::Unload => "unload",
            Self::RestartWorker => "restart_worker",
        }
    }
}

/// A standard OpenFX image-effect context.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum OfxContext {
    /// Generates imagery without a required input.
    Generator,
    /// Filters one required Source input.
    Filter,
    /// Blends required SourceFrom and SourceTo inputs.
    Transition,
    /// Paints on Source through a required Brush input.
    Paint,
    /// Retimes one required Source input.
    Retimer,
    /// Accepts an arbitrary compositing input set.
    General,
}

impl OfxContext {
    /// Every context supported by this host, in stable order.
    pub const ALL: &'static [Self] = &[
        Self::Generator,
        Self::Filter,
        Self::Transition,
        Self::Paint,
        Self::Retimer,
        Self::General,
    ];

    /// Returns the stable graph and diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Generator => "generator",
            Self::Filter => "filter",
            Self::Transition => "transition",
            Self::Paint => "paint",
            Self::Retimer => "retimer",
            Self::General => "general",
        }
    }

    /// Returns the user-facing context label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Generator => "Generator",
            Self::Filter => "Filter",
            Self::Transition => "Transition",
            Self::Paint => "Paint",
            Self::Retimer => "Retimer",
            Self::General => "General",
        }
    }
}

/// Where native plugin code executes relative to the editor process.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum OfxAdapterIsolation {
    /// Native code executes in a separately supervised worker process.
    WorkerProcess,
    /// Native code can execute in the editor process and is therefore rejected.
    InProcess,
}

/// Inspectable transport guarantees supplied by an isolated adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfxAdapterContract {
    isolation: OfxAdapterIsolation,
    protocol_revision: u32,
    max_message_bytes: usize,
    render_deadline_millis: u64,
    supports_restart: bool,
}

impl OfxAdapterContract {
    /// Describes the adapter's process and transport guarantees.
    #[must_use]
    pub const fn new(
        isolation: OfxAdapterIsolation,
        protocol_revision: u32,
        max_message_bytes: usize,
        render_deadline_millis: u64,
        supports_restart: bool,
    ) -> Self {
        Self {
            isolation,
            protocol_revision,
            max_message_bytes,
            render_deadline_millis,
            supports_restart,
        }
    }

    /// Returns the native-code isolation mode.
    #[must_use]
    pub const fn isolation(&self) -> OfxAdapterIsolation {
        self.isolation
    }

    /// Returns the versioned adapter protocol revision.
    #[must_use]
    pub const fn protocol_revision(&self) -> u32 {
        self.protocol_revision
    }

    /// Returns the maximum encoded request or response size.
    #[must_use]
    pub const fn max_message_bytes(&self) -> usize {
        self.max_message_bytes
    }

    /// Returns the adapter-enforced render deadline in milliseconds.
    #[must_use]
    pub const fn render_deadline_millis(&self) -> u64 {
        self.render_deadline_millis
    }

    /// Returns whether a failed worker can be restarted explicitly.
    #[must_use]
    pub const fn supports_restart(&self) -> bool {
        self.supports_restart
    }
}

/// The finite OpenFX subset exposed by this host implementation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfxHostCapabilities {
    api_version: SemanticVersion,
    contexts: BTreeSet<OfxContext>,
    parameter_kinds: BTreeSet<OfxParameterKind>,
    isolation: OfxAdapterIsolation,
    permissions_default_denied: bool,
}

impl OfxHostCapabilities {
    /// Returns the fixed OpenFX 1.5.1 host capability report.
    #[must_use]
    pub fn openfx_1_5_1() -> Self {
        Self {
            api_version: SemanticVersion::new(1, 5, 1),
            contexts: OfxContext::ALL.iter().copied().collect(),
            parameter_kinds: OfxParameterKind::ALL.iter().copied().collect(),
            isolation: OfxAdapterIsolation::WorkerProcess,
            permissions_default_denied: true,
        }
    }

    /// Returns the implemented OpenFX API version.
    #[must_use]
    pub const fn api_version(&self) -> &SemanticVersion {
        &self.api_version
    }

    /// Returns supported contexts in stable order.
    pub fn contexts(&self) -> impl ExactSizeIterator<Item = OfxContext> + '_ {
        self.contexts.iter().copied()
    }

    /// Returns supported finite parameter kinds in stable order.
    pub fn parameter_kinds(&self) -> impl ExactSizeIterator<Item = OfxParameterKind> + '_ {
        self.parameter_kinds.iter().copied()
    }

    /// Returns whether one parameter kind is graph-native in this host.
    #[must_use]
    pub fn supports_parameter_kind(&self, kind: OfxParameterKind) -> bool {
        self.parameter_kinds.contains(&kind)
    }

    /// Returns the required adapter isolation mode.
    #[must_use]
    pub const fn isolation(&self) -> OfxAdapterIsolation {
        self.isolation
    }

    /// Returns whether plugin permissions begin denied.
    #[must_use]
    pub const fn permissions_default_denied(&self) -> bool {
        self.permissions_default_denied
    }
}

/// A stable OpenFX plugin identity and exact schema version.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfxPluginIdentity {
    identifier: ComponentId,
    version: SemanticVersion,
}

impl OfxPluginIdentity {
    /// Creates a portable plugin identity from a canonical namespaced identifier.
    ///
    /// # Errors
    ///
    /// Returns invalid input when the identifier is not canonical namespaced text.
    pub fn new(identifier: impl AsRef<str>, version: SemanticVersion) -> Result<Self> {
        let input = identifier.as_ref();
        let identifier = ComponentId::new(input).map_err(|source| {
            ofx_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "plugin_identity",
                "invalid_plugin_identifier",
                "OpenFX plugin identifier must be a canonical namespaced identifier",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "plugin_identity")
                    .with_field("identifier", input)
                    .with_field("source", source.to_string()),
            )
        })?;
        Ok(Self {
            identifier,
            version,
        })
    }

    /// Returns the exact OpenFX plugin identifier.
    #[must_use]
    pub const fn identifier(&self) -> &ComponentId {
        &self.identifier
    }

    /// Returns the exact plugin version used as graph schema version.
    #[must_use]
    pub const fn version(&self) -> &SemanticVersion {
        &self.version
    }
}

impl fmt::Display for OfxPluginIdentity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}@{}", self.identifier, self.version)
    }
}

/// The plugin's declared ability to accept concurrent render actions.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum OfxRenderThreadSafety {
    /// Render actions must be serialized across the complete plugin.
    Unsafe,
    /// Distinct instances may render concurrently, but one instance is serialized.
    InstanceSafe,
    /// One instance may accept concurrent render actions.
    FullySafe,
}

/// Inspectable image-effect capabilities reported during describe.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfxPluginCapabilities {
    supports_tiles: bool,
    supports_multi_resolution: bool,
    temporal_clip_access: bool,
    render_thread_safety: OfxRenderThreadSafety,
    supports_gpu_render: bool,
}

impl OfxPluginCapabilities {
    /// Creates a complete plugin capability report.
    #[must_use]
    pub const fn new(
        supports_tiles: bool,
        supports_multi_resolution: bool,
        temporal_clip_access: bool,
        render_thread_safety: OfxRenderThreadSafety,
        supports_gpu_render: bool,
    ) -> Self {
        Self {
            supports_tiles,
            supports_multi_resolution,
            temporal_clip_access,
            render_thread_safety,
            supports_gpu_render,
        }
    }

    /// Returns whether the plugin can render tiles.
    #[must_use]
    pub const fn supports_tiles(&self) -> bool {
        self.supports_tiles
    }

    /// Returns whether the plugin accepts multi-resolution images.
    #[must_use]
    pub const fn supports_multi_resolution(&self) -> bool {
        self.supports_multi_resolution
    }

    /// Returns whether the plugin may request images at other times.
    #[must_use]
    pub const fn temporal_clip_access(&self) -> bool {
        self.temporal_clip_access
    }

    /// Returns the declared render concurrency safety.
    #[must_use]
    pub const fn render_thread_safety(&self) -> OfxRenderThreadSafety {
        self.render_thread_safety
    }

    /// Returns whether the worker accepts opaque GPU resource handles.
    #[must_use]
    pub const fn supports_gpu_render(&self) -> bool {
        self.supports_gpu_render
    }
}

/// Plugin-wide metadata returned by the describe action.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfxPluginDescriptor {
    identity: OfxPluginIdentity,
    label: String,
    summary: String,
    contexts: BTreeSet<OfxContext>,
    capabilities: OfxPluginCapabilities,
    requested_permissions: CapabilitySet,
}

impl OfxPluginDescriptor {
    /// Creates a complete plugin descriptor.
    ///
    /// # Errors
    ///
    /// Returns invalid input for empty metadata, no contexts, or duplicate contexts.
    pub fn new(
        identity: OfxPluginIdentity,
        label: impl Into<String>,
        summary: impl Into<String>,
        contexts: impl IntoIterator<Item = OfxContext>,
        capabilities: OfxPluginCapabilities,
        requested_permissions: CapabilitySet,
    ) -> Result<Self> {
        let label = validated_text("plugin_descriptor", "label", label.into(), MAX_TEXT_BYTES)?;
        let summary = validated_text(
            "plugin_descriptor",
            "summary",
            summary.into(),
            MAX_TEXT_BYTES,
        )?;
        let mut context_set = BTreeSet::new();
        for context in contexts {
            if !context_set.insert(context) {
                return Err(ofx_error(
                    ErrorCategory::InvalidInput,
                    Recoverability::UserCorrectable,
                    "plugin_descriptor",
                    "duplicate_context",
                    "OpenFX plugin declares one context more than once",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "plugin_descriptor")
                        .with_field("context", context.code()),
                ));
            }
        }
        if context_set.is_empty() {
            return Err(ofx_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "plugin_descriptor",
                "missing_context",
                "OpenFX plugin must declare at least one supported context",
            ));
        }
        Ok(Self {
            identity,
            label,
            summary,
            contexts: context_set,
            capabilities,
            requested_permissions,
        })
    }

    /// Returns the exact plugin identity.
    #[must_use]
    pub const fn identity(&self) -> &OfxPluginIdentity {
        &self.identity
    }

    /// Returns the user-facing plugin label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the user-facing plugin summary.
    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// Returns supported contexts in stable order.
    pub fn contexts(&self) -> impl ExactSizeIterator<Item = OfxContext> + '_ {
        self.contexts.iter().copied()
    }

    /// Returns whether the plugin supports one exact context.
    #[must_use]
    pub fn supports_context(&self, context: OfxContext) -> bool {
        self.contexts.contains(&context)
    }

    /// Returns the plugin's declared rendering capabilities.
    #[must_use]
    pub const fn capabilities(&self) -> &OfxPluginCapabilities {
        &self.capabilities
    }

    /// Returns permissions that must be granted explicitly before activation.
    #[must_use]
    pub const fn requested_permissions(&self) -> &CapabilitySet {
        &self.requested_permissions
    }
}

/// Whether one image clip is consumed or produced by an effect.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum OfxClipDirection {
    /// A read-only image input.
    Input,
    /// The write-only Output image.
    Output,
}

/// One exact OpenFX clip declaration and editor presentation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfxClipDescriptor {
    name: String,
    label: String,
    summary: String,
    direction: OfxClipDirection,
    optional: bool,
}

impl OfxClipDescriptor {
    /// Creates a bounded clip declaration while preserving the exact OpenFX name.
    ///
    /// # Errors
    ///
    /// Returns invalid input for empty, oversized, or control-bearing text.
    pub fn new(
        name: impl Into<String>,
        label: impl Into<String>,
        summary: impl Into<String>,
        direction: OfxClipDirection,
        optional: bool,
    ) -> Result<Self> {
        Ok(Self {
            name: validated_text("clip_descriptor", "name", name.into(), MAX_NAME_BYTES)?,
            label: validated_text("clip_descriptor", "label", label.into(), MAX_TEXT_BYTES)?,
            summary: validated_text("clip_descriptor", "summary", summary.into(), MAX_TEXT_BYTES)?,
            direction,
            optional,
        })
    }

    /// Returns the exact case-sensitive OpenFX clip name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the user-facing clip label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the user-facing clip summary.
    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// Returns whether this clip is read or written.
    #[must_use]
    pub const fn direction(&self) -> OfxClipDirection {
        self.direction
    }

    /// Returns whether an input clip may be unbound.
    #[must_use]
    pub const fn is_optional(&self) -> bool {
        self.optional
    }
}

/// A finite OpenFX parameter kind supported by the graph projection.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum OfxParameterKind {
    /// One signed 32-bit integer.
    Integer,
    /// Two signed 32-bit integers.
    Integer2D,
    /// Three signed 32-bit integers.
    Integer3D,
    /// One finite binary64 value.
    Double,
    /// Two finite binary64 values.
    Double2D,
    /// Three finite binary64 values.
    Double3D,
    /// Three normalized finite color values.
    Rgb,
    /// Four normalized finite color values.
    Rgba,
    /// One Boolean value.
    Boolean,
    /// One stable string-backed choice.
    Choice,
}

impl OfxParameterKind {
    /// Every graph-native parameter kind in stable order.
    pub const ALL: &'static [Self] = &[
        Self::Integer,
        Self::Integer2D,
        Self::Integer3D,
        Self::Double,
        Self::Double2D,
        Self::Double3D,
        Self::Rgb,
        Self::Rgba,
        Self::Boolean,
        Self::Choice,
    ];

    /// Returns the stable diagnostic code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Integer2D => "integer_2d",
            Self::Integer3D => "integer_3d",
            Self::Double => "double",
            Self::Double2D => "double_2d",
            Self::Double3D => "double_3d",
            Self::Rgb => "rgb",
            Self::Rgba => "rgba",
            Self::Boolean => "boolean",
            Self::Choice => "choice",
        }
    }

    fn value_type_code(self) -> &'static str {
        match self {
            Self::Integer => "superi.ofx.integer",
            Self::Integer2D => "superi.ofx.integer-2d",
            Self::Integer3D => "superi.ofx.integer-3d",
            Self::Double => "superi.ofx.double",
            Self::Double2D => "superi.ofx.double-2d",
            Self::Double3D => "superi.ofx.double-3d",
            Self::Rgb => "superi.ofx.rgb",
            Self::Rgba => "superi.ofx.rgba",
            Self::Boolean => "superi.ofx.boolean",
            Self::Choice => "superi.ofx.choice",
        }
    }
}

/// One finite host-owned OpenFX parameter sample.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum OfxParameterValue {
    /// One signed integer.
    Integer(i32),
    /// Two signed integers.
    Integer2D([i32; 2]),
    /// Three signed integers.
    Integer3D([i32; 3]),
    /// One finite double.
    Double(FiniteF64),
    /// Two finite doubles.
    Double2D([FiniteF64; 2]),
    /// Three finite doubles.
    Double3D([FiniteF64; 3]),
    /// Three finite color components.
    Rgb([FiniteF64; 3]),
    /// Four finite color components.
    Rgba([FiniteF64; 4]),
    /// One Boolean value.
    Boolean(bool),
    /// One bounded exact choice value.
    Choice(String),
}

impl OfxParameterValue {
    /// Creates one integer value.
    #[must_use]
    pub const fn integer(value: i32) -> Self {
        Self::Integer(value)
    }

    /// Creates one two-component integer value.
    #[must_use]
    pub const fn integer_2d(value: [i32; 2]) -> Self {
        Self::Integer2D(value)
    }

    /// Creates one three-component integer value.
    #[must_use]
    pub const fn integer_3d(value: [i32; 3]) -> Self {
        Self::Integer3D(value)
    }

    /// Creates one finite double value.
    ///
    /// # Errors
    ///
    /// Returns invalid input for NaN or infinity.
    pub fn double(value: f64) -> Result<Self> {
        Ok(Self::Double(FiniteF64::new(value)?))
    }

    /// Creates one finite two-component double value.
    ///
    /// # Errors
    ///
    /// Returns invalid input when a component is NaN or infinity.
    pub fn double_2d(value: [f64; 2]) -> Result<Self> {
        Ok(Self::Double2D(finite_array(value)?))
    }

    /// Creates one finite three-component double value.
    ///
    /// # Errors
    ///
    /// Returns invalid input when a component is NaN or infinity.
    pub fn double_3d(value: [f64; 3]) -> Result<Self> {
        Ok(Self::Double3D(finite_array(value)?))
    }

    /// Creates one finite RGB value.
    ///
    /// # Errors
    ///
    /// Returns invalid input when a component is NaN or infinity.
    pub fn rgb(value: [f64; 3]) -> Result<Self> {
        Ok(Self::Rgb(finite_array(value)?))
    }

    /// Creates one finite RGBA value.
    ///
    /// # Errors
    ///
    /// Returns invalid input when a component is NaN or infinity.
    pub fn rgba(value: [f64; 4]) -> Result<Self> {
        Ok(Self::Rgba(finite_array(value)?))
    }

    /// Creates one Boolean value.
    #[must_use]
    pub const fn boolean(value: bool) -> Self {
        Self::Boolean(value)
    }

    /// Creates one bounded nonempty choice value.
    ///
    /// # Errors
    ///
    /// Returns invalid input for empty, oversized, or control-bearing text.
    pub fn choice(value: impl Into<String>) -> Result<Self> {
        Ok(Self::Choice(validated_text(
            "parameter_value",
            "choice",
            value.into(),
            MAX_NAME_BYTES,
        )?))
    }

    /// Returns the exact value kind.
    #[must_use]
    pub const fn kind(&self) -> OfxParameterKind {
        match self {
            Self::Integer(_) => OfxParameterKind::Integer,
            Self::Integer2D(_) => OfxParameterKind::Integer2D,
            Self::Integer3D(_) => OfxParameterKind::Integer3D,
            Self::Double(_) => OfxParameterKind::Double,
            Self::Double2D(_) => OfxParameterKind::Double2D,
            Self::Double3D(_) => OfxParameterKind::Double3D,
            Self::Rgb(_) => OfxParameterKind::Rgb,
            Self::Rgba(_) => OfxParameterKind::Rgba,
            Self::Boolean(_) => OfxParameterKind::Boolean,
            Self::Choice(_) => OfxParameterKind::Choice,
        }
    }

    /// Returns one double sample when this has double kind.
    #[must_use]
    pub fn as_double(&self) -> Option<f64> {
        match self {
            Self::Double(value) => Some(value.get()),
            _ => None,
        }
    }

    /// Returns one choice sample when this has choice kind.
    #[must_use]
    pub fn as_choice(&self) -> Option<&str> {
        match self {
            Self::Choice(value) => Some(value),
            _ => None,
        }
    }

    fn to_graph_value<T>(&self) -> Result<GraphValue<T>> {
        match self {
            Self::Integer(value) => GraphValue::scalar(f64::from(*value)),
            Self::Integer2D(value) => GraphValue::vec2([f64::from(value[0]), f64::from(value[1])]),
            Self::Integer3D(value) => GraphValue::vec3([
                f64::from(value[0]),
                f64::from(value[1]),
                f64::from(value[2]),
            ]),
            Self::Double(value) => GraphValue::scalar(value.get()),
            Self::Double2D(value) => GraphValue::vec2(unpack_array(value)),
            Self::Double3D(value) | Self::Rgb(value) => GraphValue::vec3(unpack_array(value)),
            Self::Rgba(value) => GraphValue::color(unpack_array(value)),
            Self::Boolean(value) => Ok(GraphValue::boolean(*value)),
            Self::Choice(value) => GraphValue::choice(value.clone()),
        }
    }
}

/// One exact OpenFX parameter declaration and graph presentation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfxParameterDescriptor {
    name: String,
    label: String,
    summary: String,
    kind: OfxParameterKind,
    default_value: OfxParameterValue,
    animatable: bool,
    persistent: bool,
    choices: Vec<String>,
}

impl OfxParameterDescriptor {
    /// Creates a validated parameter declaration.
    ///
    /// Choice parameters require a unique nonempty option list containing the default. Other
    /// kinds reject options. The default value must have the declared kind.
    ///
    /// # Errors
    ///
    /// Returns invalid input for invalid text, kind mismatch, or invalid choice options.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        name: impl Into<String>,
        label: impl Into<String>,
        summary: impl Into<String>,
        kind: OfxParameterKind,
        default_value: OfxParameterValue,
        animatable: bool,
        persistent: bool,
        choices: impl IntoIterator<Item = impl Into<String>>,
    ) -> Result<Self> {
        let name = validated_text("parameter_descriptor", "name", name.into(), MAX_NAME_BYTES)?;
        let label = validated_text(
            "parameter_descriptor",
            "label",
            label.into(),
            MAX_TEXT_BYTES,
        )?;
        let summary = validated_text(
            "parameter_descriptor",
            "summary",
            summary.into(),
            MAX_TEXT_BYTES,
        )?;
        if default_value.kind() != kind {
            return Err(ofx_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "parameter_descriptor",
                "default_kind_mismatch",
                "OpenFX parameter default does not match its declared kind",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "parameter_descriptor")
                    .with_field("parameter", &name)
                    .with_field("declared_kind", kind.code())
                    .with_field("default_kind", default_value.kind().code()),
            ));
        }

        let mut choice_values = Vec::new();
        let mut unique = BTreeSet::new();
        for choice in choices {
            if choice_values.len() >= MAX_CHOICES_PER_PARAMETER {
                return Err(collection_limit_error(
                    "parameter_descriptor",
                    "choices",
                    MAX_CHOICES_PER_PARAMETER,
                ));
            }
            let choice = validated_text(
                "parameter_descriptor",
                "choice",
                choice.into(),
                MAX_NAME_BYTES,
            )?;
            if !unique.insert(choice.clone()) {
                return Err(ofx_error(
                    ErrorCategory::InvalidInput,
                    Recoverability::UserCorrectable,
                    "parameter_descriptor",
                    "duplicate_choice",
                    "OpenFX choice parameter contains a duplicate option",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "parameter_descriptor")
                        .with_field("parameter", &name)
                        .with_field("choice", &choice),
                ));
            }
            choice_values.push(choice);
        }
        match kind {
            OfxParameterKind::Choice => {
                if choice_values.is_empty() {
                    return Err(ofx_error(
                        ErrorCategory::InvalidInput,
                        Recoverability::UserCorrectable,
                        "parameter_descriptor",
                        "missing_choices",
                        "OpenFX choice parameter must declare at least one option",
                    ));
                }
                let default = default_value
                    .as_choice()
                    .expect("choice kind has choice default");
                if !unique.contains(default) {
                    return Err(ofx_error(
                        ErrorCategory::InvalidInput,
                        Recoverability::UserCorrectable,
                        "parameter_descriptor",
                        "unknown_choice_default",
                        "OpenFX choice default is absent from its option list",
                    )
                    .with_context(
                        ErrorContext::new(COMPONENT, "parameter_descriptor")
                            .with_field("parameter", &name)
                            .with_field("default", default),
                    ));
                }
            }
            _ if !choice_values.is_empty() => {
                return Err(ofx_error(
                    ErrorCategory::InvalidInput,
                    Recoverability::UserCorrectable,
                    "parameter_descriptor",
                    "unexpected_choices",
                    "non-choice OpenFX parameter cannot declare choice options",
                ));
            }
            _ => {}
        }

        Ok(Self {
            name,
            label,
            summary,
            kind,
            default_value,
            animatable,
            persistent,
            choices: choice_values,
        })
    }

    /// Returns the exact case-sensitive OpenFX parameter name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the user-facing parameter label.
    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    /// Returns the user-facing parameter summary.
    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// Returns the exact finite parameter kind.
    #[must_use]
    pub const fn kind(&self) -> OfxParameterKind {
        self.kind
    }

    /// Returns the host-owned default value.
    #[must_use]
    pub const fn default_value(&self) -> &OfxParameterValue {
        &self.default_value
    }

    /// Returns whether the plugin permits time-varying values.
    #[must_use]
    pub const fn is_animatable(&self) -> bool {
        self.animatable
    }

    /// Returns whether OFX setup persistence includes this parameter.
    #[must_use]
    pub const fn is_persistent(&self) -> bool {
        self.persistent
    }

    /// Returns exact choice values in plugin presentation order.
    #[must_use]
    pub fn choices(&self) -> &[String] {
        &self.choices
    }

    fn graph_value_type(&self) -> Result<ValueTypeId> {
        ValueTypeId::new(self.kind.value_type_code()).map_err(|source| {
            literal_error(
                "value_type",
                self.kind.value_type_code(),
                source.to_string(),
            )
        })
    }

    fn graph_control(&self) -> ParameterControl {
        match self.kind {
            OfxParameterKind::Boolean => ParameterControl::Toggle,
            OfxParameterKind::Integer | OfxParameterKind::Double => ParameterControl::Slider,
            OfxParameterKind::Double2D | OfxParameterKind::Integer2D => ParameterControl::Point2,
            OfxParameterKind::Rgb | OfxParameterKind::Rgba => ParameterControl::Color,
            OfxParameterKind::Choice => ParameterControl::Choice,
            OfxParameterKind::Integer3D | OfxParameterKind::Double3D => ParameterControl::Automatic,
        }
    }

    fn to_ofx_sample<T>(&self, value: &GraphValue<T>) -> Result<OfxParameterValue> {
        let mismatch = || {
            ofx_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "sample_parameter",
                "parameter_value_kind_mismatch",
                "sampled graph value does not match its OpenFX parameter kind",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "sample_parameter")
                    .with_field("parameter", &self.name)
                    .with_field("expected_kind", self.kind.code())
                    .with_field("actual_kind", value.kind_code()),
            )
        };
        let sampled = match self.kind {
            OfxParameterKind::Integer => {
                OfxParameterValue::Integer(exact_i32(value.as_scalar().ok_or_else(mismatch)?)?)
            }
            OfxParameterKind::Integer2D => {
                let values = value.as_vec2().ok_or_else(mismatch)?;
                OfxParameterValue::Integer2D([exact_i32(values[0])?, exact_i32(values[1])?])
            }
            OfxParameterKind::Integer3D => {
                let values = value.as_vec3().ok_or_else(mismatch)?;
                OfxParameterValue::Integer3D([
                    exact_i32(values[0])?,
                    exact_i32(values[1])?,
                    exact_i32(values[2])?,
                ])
            }
            OfxParameterKind::Double => {
                OfxParameterValue::double(value.as_scalar().ok_or_else(mismatch)?)?
            }
            OfxParameterKind::Double2D => {
                OfxParameterValue::double_2d(value.as_vec2().ok_or_else(mismatch)?)?
            }
            OfxParameterKind::Double3D => {
                OfxParameterValue::double_3d(value.as_vec3().ok_or_else(mismatch)?)?
            }
            OfxParameterKind::Rgb => OfxParameterValue::rgb(value.as_vec3().ok_or_else(mismatch)?)?,
            OfxParameterKind::Rgba => {
                OfxParameterValue::rgba(value.as_color().ok_or_else(mismatch)?)?
            }
            OfxParameterKind::Boolean => {
                OfxParameterValue::Boolean(value.as_boolean().ok_or_else(mismatch)?)
            }
            OfxParameterKind::Choice => {
                let choice = value.as_choice().ok_or_else(mismatch)?;
                if !self.choices.iter().any(|candidate| candidate == choice) {
                    return Err(ofx_error(
                        ErrorCategory::InvalidInput,
                        Recoverability::UserCorrectable,
                        "sample_parameter",
                        "unknown_choice_value",
                        "sampled OpenFX choice is absent from its descriptor",
                    )
                    .with_context(
                        ErrorContext::new(COMPONENT, "sample_parameter")
                            .with_field("parameter", &self.name)
                            .with_field("choice", choice),
                    ));
                }
                OfxParameterValue::Choice(choice.to_owned())
            }
        };
        Ok(sampled)
    }
}

/// The complete clips and parameters for one OpenFX context.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfxContextDescriptor {
    context: OfxContext,
    clips: BTreeMap<String, OfxClipDescriptor>,
    parameters: BTreeMap<String, OfxParameterDescriptor>,
    clip_graph_names: BTreeMap<String, PortName>,
    parameter_graph_names: BTreeMap<String, ParameterName>,
}

impl OfxContextDescriptor {
    /// Creates and validates one complete context description.
    ///
    /// Standard mandatory clips and host-managed parameters are checked exactly. Derived graph
    /// names must be unique, so ambiguous third-party names fail before a schema is published.
    ///
    /// # Errors
    ///
    /// Returns invalid input for duplicates, graph-name collisions, or an invalid context shape.
    pub fn new(
        context: OfxContext,
        clips: impl IntoIterator<Item = OfxClipDescriptor>,
        parameters: impl IntoIterator<Item = OfxParameterDescriptor>,
    ) -> Result<Self> {
        let mut clip_map = BTreeMap::new();
        let mut clip_graph_names = BTreeMap::new();
        let mut graph_clip_owners = BTreeMap::<String, String>::new();
        for clip in clips {
            if clip_map.len() >= MAX_CLIPS_PER_CONTEXT {
                return Err(collection_limit_error(
                    "context_descriptor",
                    "clips",
                    MAX_CLIPS_PER_CONTEXT,
                ));
            }
            let name = clip.name.clone();
            if clip_map.contains_key(&name) {
                return Err(duplicate_descriptor_error(context, "clip", &name));
            }
            let graph_name = canonical_graph_name(&name)?;
            if let Some(existing) = graph_clip_owners.insert(graph_name.clone(), name.clone()) {
                return Err(graph_name_collision_error(
                    context,
                    "clip",
                    &existing,
                    &name,
                    &graph_name,
                ));
            }
            let port_name = PortName::new(&graph_name).map_err(|source| {
                literal_error("clip_graph_name", &graph_name, source.to_string())
            })?;
            clip_graph_names.insert(name.clone(), port_name);
            clip_map.insert(name, clip);
        }

        let mut parameter_map = BTreeMap::new();
        let mut parameter_graph_names = BTreeMap::new();
        let mut graph_parameter_owners = BTreeMap::<String, String>::new();
        for parameter in parameters {
            if parameter_map.len() >= MAX_PARAMETERS_PER_CONTEXT {
                return Err(collection_limit_error(
                    "context_descriptor",
                    "parameters",
                    MAX_PARAMETERS_PER_CONTEXT,
                ));
            }
            let name = parameter.name.clone();
            if parameter_map.contains_key(&name) {
                return Err(duplicate_descriptor_error(context, "parameter", &name));
            }
            let graph_name = canonical_graph_name(&name)?;
            if let Some(existing) = graph_parameter_owners.insert(graph_name.clone(), name.clone())
            {
                return Err(graph_name_collision_error(
                    context,
                    "parameter",
                    &existing,
                    &name,
                    &graph_name,
                ));
            }
            let parameter_name = ParameterName::new(&graph_name).map_err(|source| {
                literal_error("parameter_graph_name", &graph_name, source.to_string())
            })?;
            parameter_graph_names.insert(name.clone(), parameter_name);
            parameter_map.insert(name, parameter);
        }

        validate_context_shape(context, &clip_map, &parameter_map)?;
        Ok(Self {
            context,
            clips: clip_map,
            parameters: parameter_map,
            clip_graph_names,
            parameter_graph_names,
        })
    }

    /// Returns the exact standard context.
    #[must_use]
    pub const fn context(&self) -> OfxContext {
        self.context
    }

    /// Returns clips in exact OpenFX name order.
    pub fn clips(&self) -> impl ExactSizeIterator<Item = &OfxClipDescriptor> {
        self.clips.values()
    }

    /// Looks up one exact OpenFX clip.
    #[must_use]
    pub fn clip(&self, name: &str) -> Option<&OfxClipDescriptor> {
        self.clips.get(name)
    }

    /// Returns parameters in exact OpenFX name order.
    pub fn parameters(&self) -> impl ExactSizeIterator<Item = &OfxParameterDescriptor> {
        self.parameters.values()
    }

    /// Looks up one exact OpenFX parameter.
    #[must_use]
    pub fn parameter(&self, name: &str) -> Option<&OfxParameterDescriptor> {
        self.parameters.get(name)
    }

    /// Returns the stable graph port name derived for one OpenFX clip.
    #[must_use]
    pub fn graph_clip_name(&self, name: &str) -> Option<&PortName> {
        self.clip_graph_names.get(name)
    }

    /// Returns the stable graph parameter name derived for one OpenFX parameter.
    #[must_use]
    pub fn graph_parameter_name(&self, name: &str) -> Option<&ParameterName> {
        self.parameter_graph_names.get(name)
    }

    /// Returns whether the standard context makes a parameter host-managed.
    #[must_use]
    pub fn is_host_managed_parameter(&self, name: &str) -> bool {
        matches!(
            (self.context, name),
            (OfxContext::Transition, "Transition") | (OfxContext::Retimer, "SourceTime")
        )
    }

    fn definition<T>(
        &self,
        plugin: &OfxPluginDescriptor,
    ) -> Result<EffectNodeDefinition<GraphValue<T>>> {
        let image_value = ValueTypeId::new(IMAGE_VALUE_TYPE).map_err(|source| {
            literal_error("image_value_type", IMAGE_VALUE_TYPE, source.to_string())
        })?;
        let mut inputs = Vec::new();
        let mut outputs = Vec::new();
        for clip in self.clips.values() {
            let name = self
                .clip_graph_names
                .get(clip.name())
                .expect("validated clip has graph name")
                .clone();
            let cardinality = if clip.is_optional() {
                PortCardinality::Optional
            } else {
                PortCardinality::Single
            };
            let definition = EffectPortDefinition::new(
                PortSchema::new(name, image_value.clone(), cardinality),
                clip.label(),
                clip.summary(),
            )?;
            match clip.direction() {
                OfxClipDirection::Input => inputs.push(definition),
                OfxClipDirection::Output => outputs.push(definition),
            }
        }

        let mut parameters = Vec::new();
        for parameter in self.parameters.values() {
            let name = self
                .parameter_graph_names
                .get(parameter.name())
                .expect("validated parameter has graph name")
                .clone();
            let value_type = parameter.graph_value_type()?;
            parameters.push(EffectParameterDefinition::new(
                ParameterSchema::new(
                    name,
                    value_type.clone(),
                    parameter.is_animatable() || self.is_host_managed_parameter(parameter.name()),
                ),
                parameter.label(),
                parameter.summary(),
                parameter.graph_control(),
                TypedParameterValue::new(value_type, parameter.default_value().to_graph_value()?),
            )?);
        }

        let node_type_text = format!(
            "ofx.{}.{}",
            plugin.identity().identifier().as_str(),
            self.context.code()
        );
        let node_type = NodeTypeId::new(&node_type_text)
            .map_err(|source| literal_error("node_type", &node_type_text, source.to_string()))?;
        let required_capabilities = ofx_required_capabilities(plugin.requested_permissions())?;
        EffectNodeDefinition::new(
            NodeSchemaId::new(node_type, plugin.identity().version().clone()),
            EffectMetadata::new(
                format!("{} ({})", plugin.label(), self.context.label()),
                plugin.summary(),
                "OpenFX",
            )?,
            inputs,
            outputs,
            parameters,
            NodeBehavior::new(
                if plugin.capabilities().temporal_clip_access() {
                    TimeBehavior::Unbounded
                } else {
                    TimeBehavior::CurrentFrame
                },
                RoiBehavior::Custom,
                ColorRequirements::Tagged,
                Determinism::NonDeterministic,
                CachePolicy::Disabled,
            ),
            required_capabilities,
        )
    }
}

/// A load request that distinguishes permission-free scanning from activation.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfxLoadRequest {
    scan: bool,
    granted_permissions: CapabilitySet,
}

impl OfxLoadRequest {
    fn scan() -> Self {
        Self {
            scan: true,
            granted_permissions: CapabilitySet::new(Vec::<CapabilityId>::new()),
        }
    }

    fn activation(granted_permissions: CapabilitySet) -> Self {
        Self {
            scan: false,
            granted_permissions,
        }
    }

    /// Returns whether this load may only inspect the plugin.
    #[must_use]
    pub const fn is_scan(&self) -> bool {
        self.scan
    }

    /// Returns capabilities explicitly granted for this load.
    #[must_use]
    pub const fn granted_permissions(&self) -> &CapabilitySet {
        &self.granted_permissions
    }
}

/// One finite OpenFX frame time.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OfxTime(FiniteF64);

impl OfxTime {
    /// Creates one finite time in frames.
    ///
    /// # Errors
    ///
    /// Returns invalid input for NaN or infinity.
    pub fn new(value: f64) -> Result<Self> {
        Ok(Self(FiniteF64::new(value)?))
    }

    /// Returns the frame time.
    #[must_use]
    pub fn get(self) -> f64 {
        self.0.get()
    }
}

/// Stable identity of one live plugin instance inside one graph.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct OfxInstanceKey {
    graph_id: GraphId,
    node_id: NodeId,
}

impl OfxInstanceKey {
    /// Creates an instance key from canonical graph identities.
    #[must_use]
    pub const fn new(graph_id: GraphId, node_id: NodeId) -> Self {
        Self { graph_id, node_id }
    }

    /// Returns the owning graph identity.
    #[must_use]
    pub const fn graph_id(self) -> GraphId {
        self.graph_id
    }

    /// Returns the owning node identity.
    #[must_use]
    pub const fn node_id(self) -> NodeId {
        self.node_id
    }
}

/// Owned create-instance data sent to the isolated adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfxCreateInstanceRequest {
    key: OfxInstanceKey,
    context: OfxContext,
    graph_revision: u64,
    time: OfxTime,
    parameters: BTreeMap<String, OfxParameterValue>,
}

impl OfxCreateInstanceRequest {
    /// Returns the exact graph and node instance key.
    #[must_use]
    pub const fn key(&self) -> OfxInstanceKey {
        self.key
    }

    /// Returns the immutable instance context.
    #[must_use]
    pub const fn context(&self) -> OfxContext {
        self.context
    }

    /// Returns the sampled graph revision.
    #[must_use]
    pub const fn graph_revision(&self) -> u64 {
        self.graph_revision
    }

    /// Returns the sampling time.
    #[must_use]
    pub const fn time(&self) -> OfxTime {
        self.time
    }

    /// Returns parameters by exact OpenFX name.
    #[must_use]
    pub const fn parameters(&self) -> &BTreeMap<String, OfxParameterValue> {
        &self.parameters
    }

    /// Looks up one parameter by exact OpenFX name.
    #[must_use]
    pub fn parameter(&self, name: &str) -> Option<&OfxParameterValue> {
        self.parameters.get(name)
    }
}

/// Access granted to one image resource in a render request.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum OfxImageAccess {
    /// The plugin may only read this input resource.
    ReadOnly,
    /// The plugin may only write this Output resource.
    WriteOnly,
}

/// One caller-supplied opaque image resource associated with an exact clip.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfxImageResource {
    clip_name: String,
    resource_token: String,
}

impl OfxImageResource {
    /// Creates a bounded opaque resource binding.
    ///
    /// # Errors
    ///
    /// Returns invalid input for empty, oversized, or control-bearing text.
    pub fn new(clip_name: impl Into<String>, resource_token: impl Into<String>) -> Result<Self> {
        Ok(Self {
            clip_name: validated_text(
                "image_resource",
                "clip_name",
                clip_name.into(),
                MAX_NAME_BYTES,
            )?,
            resource_token: validated_text(
                "image_resource",
                "resource_token",
                resource_token.into(),
                MAX_RESOURCE_TOKEN_BYTES,
            )?,
        })
    }

    /// Returns the exact OpenFX clip name.
    #[must_use]
    pub fn clip_name(&self) -> &str {
        &self.clip_name
    }

    /// Returns the opaque resource token resolved only by the adapter.
    #[must_use]
    pub fn resource_token(&self) -> &str {
        &self.resource_token
    }
}

/// One validated resource and its enforced access mode.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfxBoundImage {
    resource_token: String,
    access: OfxImageAccess,
}

impl OfxBoundImage {
    /// Returns the opaque resource token.
    #[must_use]
    pub fn resource_token(&self) -> &str {
        &self.resource_token
    }

    /// Returns the enforced read or write access.
    #[must_use]
    pub const fn access(&self) -> OfxImageAccess {
        self.access
    }
}

/// One nonempty integer render window in pixel coordinates.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct OfxRenderWindow {
    x1: i32,
    y1: i32,
    x2: i32,
    y2: i32,
}

impl OfxRenderWindow {
    /// Creates a render window with exclusive maximum bounds.
    ///
    /// # Errors
    ///
    /// Returns invalid input when either extent is empty or reversed.
    pub fn new(x1: i32, y1: i32, x2: i32, y2: i32) -> Result<Self> {
        if x2 <= x1 || y2 <= y1 {
            return Err(ofx_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "render_window",
                "empty_render_window",
                "OpenFX render window must have positive width and height",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "render_window")
                    .with_field("x1", x1.to_string())
                    .with_field("y1", y1.to_string())
                    .with_field("x2", x2.to_string())
                    .with_field("y2", y2.to_string()),
            ));
        }
        Ok(Self { x1, y1, x2, y2 })
    }

    /// Returns the minimum x coordinate.
    #[must_use]
    pub const fn x1(self) -> i32 {
        self.x1
    }

    /// Returns the minimum y coordinate.
    #[must_use]
    pub const fn y1(self) -> i32 {
        self.y1
    }

    /// Returns the exclusive maximum x coordinate.
    #[must_use]
    pub const fn x2(self) -> i32 {
        self.x2
    }

    /// Returns the exclusive maximum y coordinate.
    #[must_use]
    pub const fn y2(self) -> i32 {
        self.y2
    }
}

/// Owned render action data sent to the isolated adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfxRenderRequest {
    key: OfxInstanceKey,
    context: OfxContext,
    graph_revision: u64,
    time: OfxTime,
    window: OfxRenderWindow,
    parameters: BTreeMap<String, OfxParameterValue>,
    images: BTreeMap<String, OfxBoundImage>,
}

impl OfxRenderRequest {
    /// Returns the exact live instance key.
    #[must_use]
    pub const fn key(&self) -> OfxInstanceKey {
        self.key
    }

    /// Returns the immutable instance context.
    #[must_use]
    pub const fn context(&self) -> OfxContext {
        self.context
    }

    /// Returns the sampled graph revision.
    #[must_use]
    pub const fn graph_revision(&self) -> u64 {
        self.graph_revision
    }

    /// Returns the exact sampling time.
    #[must_use]
    pub const fn time(&self) -> OfxTime {
        self.time
    }

    /// Returns the requested output window.
    #[must_use]
    pub const fn window(&self) -> OfxRenderWindow {
        self.window
    }

    /// Returns parameters by exact OpenFX name.
    #[must_use]
    pub const fn parameters(&self) -> &BTreeMap<String, OfxParameterValue> {
        &self.parameters
    }

    /// Looks up one parameter by exact OpenFX name.
    #[must_use]
    pub fn parameter(&self, name: &str) -> Option<&OfxParameterValue> {
        self.parameters.get(name)
    }

    /// Returns image resources by exact OpenFX clip name.
    #[must_use]
    pub const fn images(&self) -> &BTreeMap<String, OfxBoundImage> {
        &self.images
    }

    /// Looks up one bound image by exact OpenFX clip name.
    #[must_use]
    pub fn image(&self, name: &str) -> Option<&OfxBoundImage> {
        self.images.get(name)
    }
}

/// A successful render acknowledgement from the isolated adapter.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfxRenderReceipt {
    output_resource: String,
}

impl OfxRenderReceipt {
    /// Creates an acknowledgement for one exact Output resource.
    ///
    /// # Errors
    ///
    /// Returns invalid input for an empty or oversized resource token.
    pub fn new(output_resource: impl Into<String>) -> Result<Self> {
        Ok(Self {
            output_resource: validated_text(
                "render_receipt",
                "output_resource",
                output_resource.into(),
                MAX_RESOURCE_TOKEN_BYTES,
            )?,
        })
    }

    /// Returns the resource token written by the plugin.
    #[must_use]
    pub fn output_resource(&self) -> &str {
        &self.output_resource
    }
}

/// Safe operations implemented by a supervised worker transport.
///
/// Implementations must never invoke third-party native code in the editor process. They must
/// enforce the returned [`OfxAdapterContract`], translate these owned requests to OpenFX actions in
/// the worker, and return structured errors rather than retaining hidden editable state.
pub trait IsolatedOfxAdapter: Send {
    /// Returns the adapter's inspectable isolation and transport guarantees.
    fn contract(&self) -> OfxAdapterContract;

    /// Loads the plugin for scan or explicit activation.
    fn load(&mut self, request: OfxLoadRequest) -> Result<()>;

    /// Runs the plugin-wide describe action.
    fn describe(&mut self) -> Result<OfxPluginDescriptor>;

    /// Runs describe-in-context for one declared context.
    fn describe_in_context(&mut self, context: OfxContext) -> Result<OfxContextDescriptor>;

    /// Creates one live effect instance after host parameter sampling.
    fn create_instance(&mut self, request: OfxCreateInstanceRequest) -> Result<()>;

    /// Renders one bounded output request.
    fn render(&mut self, request: OfxRenderRequest) -> Result<OfxRenderReceipt>;

    /// Destroys one live effect instance.
    fn destroy_instance(&mut self, key: OfxInstanceKey) -> Result<()>;

    /// Unloads a plugin with no remaining instances.
    fn unload(&mut self) -> Result<()>;

    /// Restarts the worker after a fault or acknowledged quarantine.
    fn restart_worker(&mut self) -> Result<()>;
}

/// Projects timeline-owned parameter literals at one explicit OpenFX time.
///
/// Graph links and expressions resolve after this projection, so timeline sampling cannot replace
/// graph dependency meaning. A literal-only consumer can return `literal.payload().clone()`.
pub trait OfxParameterSampler<T> {
    /// Samples one reached literal parameter.
    fn sample(
        &mut self,
        time: OfxTime,
        address: ParameterAddress,
        literal: &TypedParameterValue<GraphValue<T>>,
    ) -> Result<GraphValue<T>>;
}

impl<T, F> OfxParameterSampler<T> for F
where
    F: FnMut(
        OfxTime,
        ParameterAddress,
        &TypedParameterValue<GraphValue<T>>,
    ) -> Result<GraphValue<T>>,
{
    fn sample(
        &mut self,
        time: OfxTime,
        address: ParameterAddress,
        literal: &TypedParameterValue<GraphValue<T>>,
    ) -> Result<GraphValue<T>> {
        self(time, address, literal)
    }
}

/// User-visible lifecycle of one scanned plugin worker.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum OfxPluginLifecycle {
    /// Descriptors are discoverable, but native code is not loaded for use.
    Disabled,
    /// The plugin is loaded and may create or render instances.
    Ready,
    /// One adapter action failed and worker recovery is required.
    Faulted,
    /// Repeated failures require explicit quarantine acknowledgement.
    Quarantined,
}

/// Runtime state of one live effect instance.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum OfxInstanceLifecycle {
    /// The instance may accept a render action.
    Ready,
    /// A synchronous render action is active.
    Rendering,
    /// The worker failed and the instance cannot be reused.
    Failed,
}

/// Inspectable runtime state for one graph-owned plugin instance.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfxInstanceStatus {
    context: OfxContext,
    schema_id: NodeSchemaId,
    lifecycle: OfxInstanceLifecycle,
}

impl OfxInstanceStatus {
    /// Returns the immutable OpenFX context.
    #[must_use]
    pub const fn context(&self) -> OfxContext {
        self.context
    }

    /// Returns the exact graph schema instantiated by the worker.
    #[must_use]
    pub const fn schema_id(&self) -> &NodeSchemaId {
        &self.schema_id
    }

    /// Returns current instance lifecycle state.
    #[must_use]
    pub const fn lifecycle(&self) -> OfxInstanceLifecycle {
        self.lifecycle
    }
}

/// A structured, user-inspectable adapter failure summary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfxFailureRecord {
    action: OfxAction,
    category: String,
    recoverability: String,
    message: String,
}

impl OfxFailureRecord {
    fn from_error(action: OfxAction, error: &Error) -> Self {
        Self {
            action,
            category: error.category().code().to_owned(),
            recoverability: error.recoverability().code().to_owned(),
            message: error.to_string(),
        }
    }

    /// Returns the action that failed.
    #[must_use]
    pub const fn action(&self) -> OfxAction {
        self.action
    }

    /// Returns the stable Superi error category code.
    #[must_use]
    pub fn category(&self) -> &str {
        &self.category
    }

    /// Returns the stable recovery classification code.
    #[must_use]
    pub fn recoverability(&self) -> &str {
        &self.recoverability
    }

    /// Returns the retained diagnostic summary and context.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

/// One immutable plugin lifecycle and failure inspection snapshot.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfxPluginStatus {
    lifecycle: OfxPluginLifecycle,
    total_failures: u64,
    consecutive_failures: u32,
    quarantine_threshold: u32,
    last_failure: Option<OfxFailureRecord>,
    granted_permissions: CapabilitySet,
    instances: BTreeMap<OfxInstanceKey, OfxInstanceStatus>,
}

impl OfxPluginStatus {
    /// Returns current plugin lifecycle state.
    #[must_use]
    pub const fn lifecycle(&self) -> OfxPluginLifecycle {
        self.lifecycle
    }

    /// Returns all adapter failures observed by this host.
    #[must_use]
    pub const fn total_failures(&self) -> u64 {
        self.total_failures
    }

    /// Returns failures since the last healthy disable or quarantine acknowledgement.
    #[must_use]
    pub const fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    /// Returns the configured failure count that triggers quarantine.
    #[must_use]
    pub const fn quarantine_threshold(&self) -> u32 {
        self.quarantine_threshold
    }

    /// Returns the latest structured adapter failure.
    #[must_use]
    pub const fn last_failure(&self) -> Option<&OfxFailureRecord> {
        self.last_failure.as_ref()
    }

    /// Returns capabilities granted for the current activation.
    #[must_use]
    pub const fn granted_permissions(&self) -> &CapabilitySet {
        &self.granted_permissions
    }

    /// Returns current live instances in stable graph identity order.
    #[must_use]
    pub const fn instances(&self) -> &BTreeMap<OfxInstanceKey, OfxInstanceStatus> {
        &self.instances
    }
}

/// One scanned OpenFX plugin hosted through one isolated adapter.
pub struct OfxPluginHost<A> {
    adapter: A,
    adapter_contract: OfxAdapterContract,
    host_capabilities: OfxHostCapabilities,
    plugin: OfxPluginDescriptor,
    contexts: BTreeMap<OfxContext, OfxContextDescriptor>,
    lifecycle: OfxPluginLifecycle,
    granted_permissions: CapabilitySet,
    instances: BTreeMap<OfxInstanceKey, OfxInstanceStatus>,
    total_failures: u64,
    consecutive_failures: u32,
    quarantine_threshold: u32,
    last_failure: Option<OfxFailureRecord>,
}

impl<A> fmt::Debug for OfxPluginHost<A> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OfxPluginHost")
            .field("adapter_contract", &self.adapter_contract)
            .field("plugin", &self.plugin)
            .field("contexts", &self.contexts.keys().collect::<Vec<_>>())
            .field("lifecycle", &self.lifecycle)
            .field("instances", &self.instances)
            .field("total_failures", &self.total_failures)
            .field("consecutive_failures", &self.consecutive_failures)
            .field("quarantine_threshold", &self.quarantine_threshold)
            .finish_non_exhaustive()
    }
}

impl<A: IsolatedOfxAdapter> OfxPluginHost<A> {
    /// Scans and validates one plugin without granting activation permissions.
    ///
    /// The successful action order is load, describe, one describe-in-context per declared
    /// context, then unload. The resulting host is disabled and contains no live native instance.
    ///
    /// # Errors
    ///
    /// Returns unsupported for an unsafe adapter contract, invalid input for descriptors that
    /// cannot become graph-native, or the adapter's structured scan error.
    pub fn scan(mut adapter: A, quarantine_threshold: u32) -> Result<Self> {
        if quarantine_threshold == 0 {
            return Err(ofx_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "scan",
                "zero_quarantine_threshold",
                "OpenFX quarantine threshold must be positive",
            ));
        }
        let adapter_contract = adapter.contract();
        validate_adapter_contract(&adapter_contract)?;
        let host_capabilities = OfxHostCapabilities::openfx_1_5_1();
        call_adapter(&mut adapter, None, OfxAction::Load, |adapter| {
            adapter.load(OfxLoadRequest::scan())
        })?;

        let scan_result = (|| {
            let plugin = call_adapter(&mut adapter, None, OfxAction::Describe, |adapter| {
                adapter.describe()
            })?;
            let mut contexts = BTreeMap::new();
            for context in plugin.contexts() {
                let descriptor = call_adapter(
                    &mut adapter,
                    Some(plugin.identity()),
                    OfxAction::DescribeInContext,
                    |adapter| adapter.describe_in_context(context),
                )?;
                if descriptor.context() != context {
                    return Err(ofx_error(
                        ErrorCategory::InvalidInput,
                        Recoverability::UserCorrectable,
                        "scan",
                        "context_reply_mismatch",
                        "OpenFX describe-in-context reply names a different context",
                    )
                    .with_context(
                        ErrorContext::new(COMPONENT, "scan")
                            .with_field("requested_context", context.code())
                            .with_field("actual_context", descriptor.context().code()),
                    ));
                }
                let _ = descriptor.definition::<()>(&plugin)?;
                contexts.insert(context, descriptor);
            }
            Ok((plugin, contexts))
        })();
        let unload_result = call_adapter(&mut adapter, None, OfxAction::Unload, |adapter| {
            adapter.unload()
        });
        let (plugin, contexts) = match scan_result {
            Ok(scanned) => {
                unload_result?;
                scanned
            }
            Err(mut error) => {
                if let Err(unload_error) = unload_result {
                    error.push_context(
                        ErrorContext::new(COMPONENT, "scan_cleanup")
                            .with_field("unload_error", unload_error.to_string()),
                    );
                }
                return Err(error);
            }
        };

        Ok(Self {
            adapter,
            adapter_contract,
            host_capabilities,
            plugin,
            contexts,
            lifecycle: OfxPluginLifecycle::Disabled,
            granted_permissions: CapabilitySet::new(Vec::<CapabilityId>::new()),
            instances: BTreeMap::new(),
            total_failures: 0,
            consecutive_failures: 0,
            quarantine_threshold,
            last_failure: None,
        })
    }

    /// Returns the validated adapter contract.
    #[must_use]
    pub const fn adapter_contract(&self) -> &OfxAdapterContract {
        &self.adapter_contract
    }

    /// Returns the fixed host capability report.
    #[must_use]
    pub const fn capabilities(&self) -> &OfxHostCapabilities {
        &self.host_capabilities
    }

    /// Returns the scanned plugin descriptor.
    #[must_use]
    pub const fn plugin(&self) -> &OfxPluginDescriptor {
        &self.plugin
    }

    /// Returns scanned context descriptors in stable context order.
    pub fn contexts(&self) -> impl ExactSizeIterator<Item = &OfxContextDescriptor> {
        self.contexts.values()
    }

    /// Looks up one scanned context descriptor.
    #[must_use]
    pub fn context(&self, context: OfxContext) -> Option<&OfxContextDescriptor> {
        self.contexts.get(&context)
    }

    /// Returns current plugin lifecycle state.
    #[must_use]
    pub const fn lifecycle(&self) -> OfxPluginLifecycle {
        self.lifecycle
    }

    /// Captures current lifecycle, failures, and instances for user inspection.
    #[must_use]
    pub fn status(&self) -> OfxPluginStatus {
        OfxPluginStatus {
            lifecycle: self.lifecycle,
            total_failures: self.total_failures,
            consecutive_failures: self.consecutive_failures,
            quarantine_threshold: self.quarantine_threshold,
            last_failure: self.last_failure.clone(),
            granted_permissions: self.granted_permissions.clone(),
            instances: self.instances.clone(),
        }
    }

    /// Builds one graph-native definition from a scanned context.
    ///
    /// # Errors
    ///
    /// Returns not found when the plugin did not declare the context.
    pub fn definition<T>(
        &self,
        context: OfxContext,
    ) -> Result<EffectNodeDefinition<GraphValue<T>>> {
        self.contexts
            .get(&context)
            .ok_or_else(|| missing_context_error(self.plugin.identity(), context))?
            .definition(&self.plugin)
    }

    /// Builds the complete editor discovery catalog in stable context order.
    ///
    /// The catalog remains populated while the plugin is disabled, faulted, or quarantined so
    /// authored operations stay understandable and editable.
    ///
    /// # Errors
    ///
    /// Returns a schema construction or catalog conflict error.
    pub fn discovered_catalog<T>(&self) -> Result<EffectCatalog<GraphValue<T>>> {
        let mut catalog = EffectCatalog::new();
        catalog.register_batch(
            self.contexts
                .keys()
                .copied()
                .map(|context| self.definition(context))
                .collect::<Result<Vec<_>>>()?,
        )?;
        Ok(catalog)
    }

    /// Builds schemas currently available to runtime evaluation.
    ///
    /// Only a ready plugin contributes definitions. Other lifecycle states return an empty catalog
    /// so existing graph missing-node resolution can retain authored state without executing code.
    ///
    /// # Errors
    ///
    /// Returns a schema construction or catalog conflict error.
    pub fn active_catalog<T>(&self) -> Result<EffectCatalog<GraphValue<T>>> {
        if self.lifecycle == OfxPluginLifecycle::Ready {
            self.discovered_catalog()
        } else {
            Ok(EffectCatalog::new())
        }
    }

    /// Explicitly grants permissions and loads a disabled plugin for use.
    ///
    /// # Errors
    ///
    /// Returns permission denied for a missing grant, conflict outside Disabled state, or the
    /// adapter load failure.
    pub fn enable(&mut self, granted_permissions: &CapabilitySet) -> Result<()> {
        self.require_lifecycle("enable", &[OfxPluginLifecycle::Disabled])?;
        if !granted_permissions.contains_all(self.plugin.requested_permissions()) {
            let missing = self
                .plugin
                .requested_permissions()
                .iter()
                .filter(|capability| !granted_permissions.contains(capability))
                .map(|capability| capability.as_str())
                .collect::<Vec<_>>()
                .join(",");
            return Err(ofx_error(
                ErrorCategory::PermissionDenied,
                Recoverability::UserCorrectable,
                "enable",
                "missing_permission_grant",
                "OpenFX plugin activation requires explicit capability grants",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "enable")
                    .with_field("plugin", self.plugin.identity().to_string())
                    .with_field("missing_permissions", missing),
            ));
        }
        let request = OfxLoadRequest::activation(granted_permissions.clone());
        self.invoke_adapter(OfxAction::Load, |adapter| adapter.load(request))?;
        self.lifecycle = OfxPluginLifecycle::Ready;
        self.granted_permissions = granted_permissions.clone();
        Ok(())
    }

    /// Unloads a ready plugin after every instance is destroyed.
    ///
    /// # Errors
    ///
    /// Returns conflict when the plugin is not ready or instances remain, or the adapter failure.
    pub fn disable(&mut self) -> Result<()> {
        self.require_lifecycle("disable", &[OfxPluginLifecycle::Ready])?;
        if !self.instances.is_empty() {
            return Err(ofx_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "disable",
                "live_instances",
                "OpenFX plugin cannot unload while live instances remain",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "disable")
                    .with_field("instance_count", self.instances.len().to_string()),
            ));
        }
        self.invoke_adapter(OfxAction::Unload, IsolatedOfxAdapter::unload)?;
        self.lifecycle = OfxPluginLifecycle::Disabled;
        self.granted_permissions = CapabilitySet::new(Vec::<CapabilityId>::new());
        self.consecutive_failures = 0;
        Ok(())
    }

    /// Restarts one faulted worker without clearing its consecutive failure count.
    ///
    /// All worker instances are discarded because their native handles cannot survive restart.
    ///
    /// # Errors
    ///
    /// Returns conflict unless the plugin is faulted, or the adapter restart failure.
    pub fn recover(&mut self) -> Result<()> {
        self.require_lifecycle("recover", &[OfxPluginLifecycle::Faulted])?;
        self.invoke_adapter(OfxAction::RestartWorker, IsolatedOfxAdapter::restart_worker)?;
        self.instances.clear();
        self.granted_permissions = CapabilitySet::new(Vec::<CapabilityId>::new());
        self.lifecycle = OfxPluginLifecycle::Disabled;
        Ok(())
    }

    /// Acknowledges quarantine, restarts the worker, and clears the consecutive count.
    ///
    /// Total failure history and the last structured failure remain inspectable.
    ///
    /// # Errors
    ///
    /// Returns conflict unless the plugin is quarantined, or the adapter restart failure.
    pub fn clear_quarantine(&mut self) -> Result<()> {
        self.require_lifecycle("clear_quarantine", &[OfxPluginLifecycle::Quarantined])?;
        self.invoke_adapter(OfxAction::RestartWorker, IsolatedOfxAdapter::restart_worker)?;
        self.instances.clear();
        self.granted_permissions = CapabilitySet::new(Vec::<CapabilityId>::new());
        self.consecutive_failures = 0;
        self.lifecycle = OfxPluginLifecycle::Disabled;
        Ok(())
    }

    /// Creates one plugin instance from an exact graph revision and explicit-time sampler.
    ///
    /// # Errors
    ///
    /// Returns graph or descriptor validation errors before invoking the adapter, conflict for an
    /// unavailable or duplicate instance, or the adapter create failure.
    pub fn create_instance<T: Clone>(
        &mut self,
        context: OfxContext,
        snapshot: &GraphSnapshot<GraphValue<T>>,
        node_id: NodeId,
        time: OfxTime,
        sampler: &mut impl OfxParameterSampler<T>,
    ) -> Result<()> {
        self.require_lifecycle("create_instance", &[OfxPluginLifecycle::Ready])?;
        let key = OfxInstanceKey::new(snapshot.graph_id(), node_id);
        if self.instances.contains_key(&key) {
            return Err(ofx_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "create_instance",
                "duplicate_instance",
                "OpenFX instance already exists for this graph node",
            ));
        }
        let descriptor = self
            .contexts
            .get(&context)
            .cloned()
            .ok_or_else(|| missing_context_error(self.plugin.identity(), context))?;
        let definition = descriptor.definition::<T>(&self.plugin)?;
        validate_node_schema(
            snapshot,
            node_id,
            definition.schema().id(),
            definition.schema(),
        )?;
        let parameters = sample_parameters(&descriptor, snapshot, node_id, time, sampler)?;
        let request = OfxCreateInstanceRequest {
            key,
            context,
            graph_revision: snapshot.revision(),
            time,
            parameters,
        };
        self.invoke_adapter(OfxAction::CreateInstance, |adapter| {
            adapter.create_instance(request)
        })?;
        self.instances.insert(
            key,
            OfxInstanceStatus {
                context,
                schema_id: definition.schema().id().clone(),
                lifecycle: OfxInstanceLifecycle::Ready,
            },
        );
        Ok(())
    }

    /// Renders one live instance from an exact graph revision and bounded image bindings.
    ///
    /// # Errors
    ///
    /// Returns graph, parameter, or resource errors before adapter invocation; conflict for an
    /// unavailable instance; or a recorded adapter render failure.
    #[allow(clippy::too_many_arguments)]
    pub fn render<T: Clone>(
        &mut self,
        key: OfxInstanceKey,
        snapshot: &GraphSnapshot<GraphValue<T>>,
        time: OfxTime,
        sampler: &mut impl OfxParameterSampler<T>,
        window: OfxRenderWindow,
        images: impl IntoIterator<Item = OfxImageResource>,
    ) -> Result<OfxRenderReceipt> {
        self.require_lifecycle("render", &[OfxPluginLifecycle::Ready])?;
        if snapshot.graph_id() != key.graph_id() {
            return Err(ofx_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "render",
                "graph_identity_mismatch",
                "OpenFX render snapshot does not own the live instance",
            ));
        }
        let status = self.instances.get(&key).cloned().ok_or_else(|| {
            ofx_error(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "render",
                "missing_instance",
                "OpenFX live instance does not exist",
            )
        })?;
        if status.lifecycle != OfxInstanceLifecycle::Ready {
            return Err(ofx_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "render",
                "instance_not_ready",
                "OpenFX instance is not ready to render",
            ));
        }
        let descriptor = self
            .contexts
            .get(&status.context)
            .cloned()
            .expect("live instance context remains scanned");
        let definition = descriptor.definition::<T>(&self.plugin)?;
        validate_node_schema(
            snapshot,
            key.node_id(),
            &status.schema_id,
            definition.schema(),
        )?;
        let parameters = sample_parameters(&descriptor, snapshot, key.node_id(), time, sampler)?;
        let bound_images = bind_images(&descriptor, images)?;
        let output_resource = bound_images
            .get("Output")
            .expect("validated context has bound Output")
            .resource_token
            .clone();
        let request = OfxRenderRequest {
            key,
            context: status.context,
            graph_revision: snapshot.revision(),
            time,
            window,
            parameters,
            images: bound_images,
        };
        self.instances
            .get_mut(&key)
            .expect("validated live instance remains present")
            .lifecycle = OfxInstanceLifecycle::Rendering;
        let result = self.invoke_adapter(OfxAction::Render, |adapter| adapter.render(request));
        let receipt = result?;
        if receipt.output_resource() != output_resource {
            let error = ofx_error(
                ErrorCategory::CorruptData,
                Recoverability::Terminal,
                "render",
                "output_receipt_mismatch",
                "OpenFX adapter acknowledged a different Output resource",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "render")
                    .with_field("expected_output", output_resource)
                    .with_field("actual_output", receipt.output_resource()),
            );
            self.record_failure(OfxAction::Render, &error);
            return Err(error);
        }
        self.instances
            .get_mut(&key)
            .expect("successful live instance remains present")
            .lifecycle = OfxInstanceLifecycle::Ready;
        Ok(receipt)
    }

    /// Destroys one ready instance before plugin unload.
    ///
    /// # Errors
    ///
    /// Returns not found for an unknown key, conflict for a non-ready instance, or the adapter
    /// destroy failure.
    pub fn destroy_instance(&mut self, key: OfxInstanceKey) -> Result<()> {
        self.require_lifecycle("destroy_instance", &[OfxPluginLifecycle::Ready])?;
        let status = self.instances.get(&key).ok_or_else(|| {
            ofx_error(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "destroy_instance",
                "missing_instance",
                "OpenFX live instance does not exist",
            )
        })?;
        if status.lifecycle != OfxInstanceLifecycle::Ready {
            return Err(ofx_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "destroy_instance",
                "instance_not_ready",
                "OpenFX instance cannot be destroyed during an active or failed action",
            ));
        }
        self.invoke_adapter(OfxAction::DestroyInstance, |adapter| {
            adapter.destroy_instance(key)
        })?;
        self.instances.remove(&key);
        Ok(())
    }

    fn require_lifecycle(
        &self,
        operation: &'static str,
        allowed: &[OfxPluginLifecycle],
    ) -> Result<()> {
        if allowed.contains(&self.lifecycle) {
            return Ok(());
        }
        Err(ofx_error(
            ErrorCategory::Conflict,
            Recoverability::UserCorrectable,
            operation,
            "invalid_lifecycle",
            "OpenFX plugin lifecycle does not permit this operation",
        )
        .with_context(
            ErrorContext::new(COMPONENT, operation)
                .with_field("plugin", self.plugin.identity().to_string())
                .with_field("lifecycle", lifecycle_code(self.lifecycle)),
        ))
    }

    fn invoke_adapter<R>(
        &mut self,
        action: OfxAction,
        invoke: impl FnOnce(&mut A) -> Result<R>,
    ) -> Result<R> {
        match call_adapter(
            &mut self.adapter,
            Some(self.plugin.identity()),
            action,
            invoke,
        ) {
            Ok(value) => Ok(value),
            Err(error) => {
                self.record_failure(action, &error);
                Err(error)
            }
        }
    }

    fn record_failure(&mut self, action: OfxAction, error: &Error) {
        self.total_failures = self.total_failures.saturating_add(1);
        self.consecutive_failures = self.consecutive_failures.saturating_add(1);
        self.last_failure = Some(OfxFailureRecord::from_error(action, error));
        for status in self.instances.values_mut() {
            status.lifecycle = OfxInstanceLifecycle::Failed;
        }
        self.lifecycle = if self.consecutive_failures >= self.quarantine_threshold {
            OfxPluginLifecycle::Quarantined
        } else {
            OfxPluginLifecycle::Faulted
        };
    }
}

fn validate_adapter_contract(contract: &OfxAdapterContract) -> Result<()> {
    let invalid = match contract.isolation() {
        OfxAdapterIsolation::WorkerProcess => None,
        OfxAdapterIsolation::InProcess => Some("in_process_native_code"),
    }
    .or_else(|| {
        (contract.protocol_revision() != ADAPTER_PROTOCOL_REVISION)
            .then_some("unsupported_protocol_revision")
    })
    .or_else(|| (contract.max_message_bytes() == 0).then_some("unbounded_messages"))
    .or_else(|| (contract.render_deadline_millis() == 0).then_some("missing_render_deadline"))
    .or_else(|| (!contract.supports_restart()).then_some("worker_restart_unavailable"));
    if let Some(reason) = invalid {
        return Err(ofx_error(
            ErrorCategory::Unsupported,
            Recoverability::UserCorrectable,
            "validate_adapter",
            reason,
            "OpenFX adapter does not satisfy the isolated worker contract",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "validate_adapter")
                .with_field(
                    "protocol_revision",
                    contract.protocol_revision().to_string(),
                )
                .with_field(
                    "max_message_bytes",
                    contract.max_message_bytes().to_string(),
                )
                .with_field(
                    "render_deadline_millis",
                    contract.render_deadline_millis().to_string(),
                )
                .with_field("supports_restart", contract.supports_restart().to_string()),
        ));
    }
    Ok(())
}

fn validate_context_shape(
    context: OfxContext,
    clips: &BTreeMap<String, OfxClipDescriptor>,
    parameters: &BTreeMap<String, OfxParameterDescriptor>,
) -> Result<()> {
    let output_count = clips
        .values()
        .filter(|clip| clip.direction() == OfxClipDirection::Output)
        .count();
    require_clip(context, clips, "Output", OfxClipDirection::Output, false)?;
    if output_count != 1 {
        return Err(context_shape_error(
            context,
            "output_count",
            "OpenFX context must declare exactly one Output clip",
        ));
    }

    let mandatory_inputs: &[&str] = match context {
        OfxContext::Generator | OfxContext::General => &[],
        OfxContext::Filter | OfxContext::Retimer => &["Source"],
        OfxContext::Transition => &["SourceFrom", "SourceTo"],
        OfxContext::Paint => &["Source", "Brush"],
    };
    for name in mandatory_inputs {
        require_clip(context, clips, name, OfxClipDirection::Input, false)?;
    }
    if context != OfxContext::General {
        for clip in clips.values().filter(|clip| {
            clip.direction() == OfxClipDirection::Input && !mandatory_inputs.contains(&clip.name())
        }) {
            if !clip.is_optional() {
                return Err(context_shape_error(
                    context,
                    "required_extra_input",
                    "extra input clips must be optional in this OpenFX context",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "context_descriptor")
                        .with_field("clip", clip.name()),
                ));
            }
        }
    }
    match context {
        OfxContext::Transition => {
            require_parameter(context, parameters, "Transition", OfxParameterKind::Double)?
        }
        OfxContext::Retimer => {
            require_parameter(context, parameters, "SourceTime", OfxParameterKind::Double)?
        }
        _ => {}
    }
    Ok(())
}

fn require_clip(
    context: OfxContext,
    clips: &BTreeMap<String, OfxClipDescriptor>,
    name: &str,
    direction: OfxClipDirection,
    optional: bool,
) -> Result<()> {
    let Some(clip) = clips.get(name) else {
        return Err(context_shape_error(
            context,
            "missing_mandatory_clip",
            "OpenFX context is missing a mandatory clip",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "context_descriptor").with_field("clip", name),
        ));
    };
    if clip.direction() != direction || clip.is_optional() != optional {
        return Err(context_shape_error(
            context,
            "invalid_mandatory_clip",
            "OpenFX mandatory clip has invalid direction or optionality",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "context_descriptor").with_field("clip", name),
        ));
    }
    Ok(())
}

fn require_parameter(
    context: OfxContext,
    parameters: &BTreeMap<String, OfxParameterDescriptor>,
    name: &str,
    kind: OfxParameterKind,
) -> Result<()> {
    let Some(parameter) = parameters.get(name) else {
        return Err(context_shape_error(
            context,
            "missing_mandatory_parameter",
            "OpenFX context is missing a host-managed parameter",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "context_descriptor").with_field("parameter", name),
        ));
    };
    if parameter.kind() != kind {
        return Err(context_shape_error(
            context,
            "invalid_mandatory_parameter",
            "OpenFX host-managed parameter has the wrong kind",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "context_descriptor")
                .with_field("parameter", name)
                .with_field("expected_kind", kind.code())
                .with_field("actual_kind", parameter.kind().code()),
        ));
    }
    Ok(())
}

fn sample_parameters<T: Clone>(
    descriptor: &OfxContextDescriptor,
    snapshot: &GraphSnapshot<GraphValue<T>>,
    node_id: NodeId,
    time: OfxTime,
    sampler: &mut impl OfxParameterSampler<T>,
) -> Result<BTreeMap<String, OfxParameterValue>> {
    let node = snapshot.node(node_id).ok_or_else(|| {
        ofx_error(
            ErrorCategory::NotFound,
            Recoverability::UserCorrectable,
            "sample_parameters",
            "missing_graph_node",
            "OpenFX graph node does not exist",
        )
    })?;
    let mut samples = BTreeMap::new();
    for parameter in descriptor.parameters.values() {
        let graph_name = descriptor
            .parameter_graph_names
            .get(parameter.name())
            .expect("validated parameter has graph name");
        let editable = node
            .parameters()
            .values()
            .find(|editable| editable.name() == graph_name)
            .expect("validated exact schema binds every parameter");
        let address = ParameterAddress::new(node_id, editable.id());
        let evaluated = snapshot.evaluate_parameter_with(address, |address, literal| {
            sampler.sample(time, address, literal)
        })?;
        samples.insert(
            parameter.name().to_owned(),
            parameter.to_ofx_sample(evaluated.value().payload())?,
        );
    }
    Ok(samples)
}

fn validate_node_schema<T>(
    snapshot: &GraphSnapshot<T>,
    node_id: NodeId,
    expected_id: &NodeSchemaId,
    expected_schema: &superi_graph::node::NodeSchema,
) -> Result<()> {
    let node = snapshot.node(node_id).ok_or_else(|| {
        ofx_error(
            ErrorCategory::NotFound,
            Recoverability::UserCorrectable,
            "validate_instance",
            "missing_graph_node",
            "OpenFX graph node does not exist",
        )
    })?;
    if node.schema().id() != expected_id || node.schema() != expected_schema {
        return Err(ofx_error(
            ErrorCategory::Conflict,
            Recoverability::UserCorrectable,
            "validate_instance",
            "schema_mismatch",
            "OpenFX graph node does not match the scanned exact schema",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "validate_instance")
                .with_field("node_id", node_id.to_string())
                .with_field("expected_schema", expected_id.to_string())
                .with_field("actual_schema", node.schema().id().to_string()),
        ));
    }
    Ok(())
}

fn bind_images(
    descriptor: &OfxContextDescriptor,
    resources: impl IntoIterator<Item = OfxImageResource>,
) -> Result<BTreeMap<String, OfxBoundImage>> {
    let mut images = BTreeMap::new();
    for resource in resources {
        if images.len() >= MAX_CLIPS_PER_CONTEXT {
            return Err(collection_limit_error(
                "bind_images",
                "image_resources",
                MAX_CLIPS_PER_CONTEXT,
            ));
        }
        let clip = descriptor.clip(&resource.clip_name).ok_or_else(|| {
            ofx_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "bind_images",
                "unknown_clip",
                "OpenFX image resource names an unknown clip",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "bind_images").with_field("clip", &resource.clip_name),
            )
        })?;
        let name = resource.clip_name;
        let bound = OfxBoundImage {
            resource_token: resource.resource_token,
            access: match clip.direction() {
                OfxClipDirection::Input => OfxImageAccess::ReadOnly,
                OfxClipDirection::Output => OfxImageAccess::WriteOnly,
            },
        };
        if images.insert(name.clone(), bound).is_some() {
            return Err(ofx_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "bind_images",
                "duplicate_clip_resource",
                "OpenFX image clip is bound more than once",
            )
            .with_context(ErrorContext::new(COMPONENT, "bind_images").with_field("clip", name)));
        }
    }
    for clip in descriptor.clips.values().filter(|clip| !clip.is_optional()) {
        if !images.contains_key(clip.name()) {
            return Err(ofx_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "bind_images",
                "missing_clip_resource",
                "OpenFX render is missing a required image resource",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "bind_images").with_field("clip", clip.name()),
            ));
        }
    }
    Ok(images)
}

fn call_adapter<A, R>(
    adapter: &mut A,
    plugin: Option<&OfxPluginIdentity>,
    action: OfxAction,
    invoke: impl FnOnce(&mut A) -> Result<R>,
) -> Result<R> {
    let result = catch_unwind(AssertUnwindSafe(|| invoke(adapter))).map_err(|_| {
        ofx_error(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "adapter_action",
            "adapter_panic",
            "isolated OpenFX adapter panicked while handling an action",
        )
    })?;
    result.map_err(|mut error| {
        let mut context =
            ErrorContext::new(COMPONENT, "adapter_action").with_field("action", action.code());
        if let Some(plugin) = plugin {
            context.insert_field("plugin", plugin.to_string());
        }
        error.push_context(context);
        error
    })
}

fn ofx_required_capabilities(permissions: &CapabilitySet) -> Result<CapabilitySet> {
    let ofx = CapabilityId::new(OFX_CAPABILITY)
        .map_err(|source| literal_error("capability", OFX_CAPABILITY, source.to_string()))?;
    let worker = CapabilityId::new(WORKER_CAPABILITY)
        .map_err(|source| literal_error("capability", WORKER_CAPABILITY, source.to_string()))?;
    Ok(CapabilitySet::new(
        [ofx, worker].into_iter().chain(permissions.iter().cloned()),
    ))
}

fn canonical_graph_name(name: &str) -> Result<String> {
    let mut output = String::new();
    let mut previous_lower_or_digit = false;
    for byte in name.bytes() {
        match byte {
            b'A'..=b'Z' => {
                if previous_lower_or_digit && !output.ends_with('-') {
                    output.push('-');
                }
                output.push(char::from(byte.to_ascii_lowercase()));
                previous_lower_or_digit = false;
            }
            b'a'..=b'z' => {
                output.push(char::from(byte));
                previous_lower_or_digit = true;
            }
            b'0'..=b'9' => {
                output.push(char::from(byte));
                previous_lower_or_digit = true;
            }
            _ => {
                if !output.is_empty() && !output.ends_with('-') {
                    output.push('-');
                }
                previous_lower_or_digit = false;
            }
        }
    }
    while output.ends_with('-') {
        output.pop();
    }
    if output.is_empty() {
        return Err(ofx_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "canonical_graph_name",
            "empty_graph_name",
            "OpenFX name cannot produce a portable graph field name",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "canonical_graph_name").with_field("ofx_name", name),
        ));
    }
    if output.as_bytes()[0].is_ascii_digit() {
        output.insert_str(0, "field-");
    }
    Ok(output)
}

fn finite_array<const N: usize>(values: [f64; N]) -> Result<[FiniteF64; N]> {
    let mut output = [FiniteF64::new(0.0)?; N];
    for (destination, source) in output.iter_mut().zip(values) {
        *destination = FiniteF64::new(source)?;
    }
    Ok(output)
}

fn unpack_array<const N: usize>(values: &[FiniteF64; N]) -> [f64; N] {
    let mut output = [0.0; N];
    for (destination, source) in output.iter_mut().zip(values) {
        *destination = source.get();
    }
    output
}

fn exact_i32(value: f64) -> Result<i32> {
    if value.fract() == 0.0 && value >= f64::from(i32::MIN) && value <= f64::from(i32::MAX) {
        Ok(value as i32)
    } else {
        Err(ofx_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "sample_parameter",
            "invalid_integer_sample",
            "OpenFX integer parameter sample must be an exact signed 32-bit integer",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "sample_parameter").with_field("value", value.to_string()),
        ))
    }
}

fn validated_text(
    operation: &'static str,
    field: &'static str,
    value: String,
    max_bytes: usize,
) -> Result<String> {
    let reason = if value.trim().is_empty() {
        Some("empty_text")
    } else if value.len() > max_bytes {
        Some("text_too_long")
    } else if value.chars().any(char::is_control) {
        Some("control_character")
    } else {
        None
    };
    if let Some(reason) = reason {
        return Err(ofx_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            operation,
            reason,
            "OpenFX descriptor text is empty, oversized, or contains control characters",
        )
        .with_context(
            ErrorContext::new(COMPONENT, operation)
                .with_field("field", field)
                .with_field("bytes", value.len().to_string())
                .with_field("max_bytes", max_bytes.to_string()),
        ));
    }
    Ok(value)
}

fn collection_limit_error(
    operation: &'static str,
    collection: &'static str,
    limit: usize,
) -> Error {
    ofx_error(
        ErrorCategory::ResourceExhausted,
        Recoverability::UserCorrectable,
        operation,
        "collection_limit",
        "OpenFX descriptor or request exceeds a supported collection limit",
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("collection", collection)
            .with_field("limit", limit.to_string()),
    )
}

fn duplicate_descriptor_error(context: OfxContext, kind: &str, name: &str) -> Error {
    ofx_error(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        "context_descriptor",
        "duplicate_descriptor",
        "OpenFX context contains a duplicate descriptor name",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "context_descriptor")
            .with_field("context", context.code())
            .with_field("kind", kind)
            .with_field("name", name),
    )
}

fn graph_name_collision_error(
    context: OfxContext,
    kind: &str,
    first: &str,
    second: &str,
    graph_name: &str,
) -> Error {
    ofx_error(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        "context_descriptor",
        "graph_name_collision",
        "distinct OpenFX names produce the same portable graph field name",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "context_descriptor")
            .with_field("context", context.code())
            .with_field("kind", kind)
            .with_field("first_name", first)
            .with_field("second_name", second)
            .with_field("graph_name", graph_name),
    )
}

fn context_shape_error(context: OfxContext, reason: &'static str, message: &'static str) -> Error {
    ofx_error(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        "context_descriptor",
        reason,
        message,
    )
    .with_context(
        ErrorContext::new(COMPONENT, "context_descriptor").with_field("context", context.code()),
    )
}

fn missing_context_error(identity: &OfxPluginIdentity, context: OfxContext) -> Error {
    ofx_error(
        ErrorCategory::NotFound,
        Recoverability::UserCorrectable,
        "context",
        "unsupported_context",
        "OpenFX plugin did not declare the requested context",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "context")
            .with_field("plugin", identity.to_string())
            .with_field("context", context.code()),
    )
}

fn literal_error(field: &'static str, value: &str, source: String) -> Error {
    ofx_error(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        "build_definition",
        "invalid_repository_literal",
        "repository-owned OpenFX graph literal is invalid",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "build_definition")
            .with_field("field", field)
            .with_field("value", value)
            .with_field("source", source),
    )
}

fn lifecycle_code(lifecycle: OfxPluginLifecycle) -> &'static str {
    match lifecycle {
        OfxPluginLifecycle::Disabled => "disabled",
        OfxPluginLifecycle::Ready => "ready",
        OfxPluginLifecycle::Faulted => "faulted",
        OfxPluginLifecycle::Quarantined => "quarantined",
    }
}

fn ofx_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    reason: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation).with_field("reason", reason))
}
