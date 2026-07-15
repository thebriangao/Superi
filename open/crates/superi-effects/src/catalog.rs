//! Stable built-in schemas for editable visual operations.

use std::collections::BTreeMap;

use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::settings::{CapabilityId, CapabilitySet, SemanticVersion};
use superi_graph::mutate::{EditableNode, TypedParameterValue};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeRegistry, NodeSchema,
    NodeSchemaId, NodeTypeId, ParameterName, ParameterSchema, PortCardinality, PortName,
    PortSchema, RoiBehavior, TimeBehavior, ValueTypeId,
};
use superi_graph::value::GraphValue;

pub use crate::authoring::{EffectInstanceBindings as EffectNodeBindings, EffectParameterBinding};
use crate::authoring::{
    EffectMetadata, EffectNodeDefinition, EffectParameterDefinition, EffectPortDefinition,
    ParameterControl,
};

const COMPONENT: &str = "superi-effects.catalog";
const IMAGE_VALUE: &str = "superi.value.image";
const SCALAR_VALUE: &str = "superi.value.scalar";
const COLOR_VALUE: &str = "superi.value.color";
const CHOICE_VALUE: &str = "superi.value.choice";

/// Stable catalog families for discovery and editor grouping.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum EffectNodeFamily {
    /// Spatial transforms and bounds operations.
    Geometry,
    /// Alpha, blend, and compositing operations.
    Compositing,
    /// Neighborhood filters and distortion.
    Filter,
    /// Color-derived keying operations.
    Keying,
    /// Reusable pointwise color utilities.
    Utility,
}

/// Every built-in visual operation supplied by this checkpoint.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum EffectNodeKind {
    /// Projective 3 by 3 image transform.
    Transform,
    /// Pixel-inset crop.
    Crop,
    /// Premultiplied opacity.
    Opacity,
    /// Separable color blend followed by source-over.
    Blend,
    /// Porter-Duff compositing.
    Composite,
    /// Gaussian blur.
    GaussianBlur,
    /// Unsharp-mask sharpening.
    Sharpen,
    /// Radial lens distortion.
    RadialDistortion,
    /// Chroma key with spill suppression.
    ChromaKey,
    /// Pointwise color inversion.
    Invert,
    /// Per-channel gain and offset.
    Grade,
}

impl EffectNodeKind {
    /// Every built-in kind in stable catalog presentation order.
    pub const ALL: &'static [Self] = &[
        Self::Transform,
        Self::Crop,
        Self::Opacity,
        Self::Blend,
        Self::Composite,
        Self::GaussianBlur,
        Self::Sharpen,
        Self::RadialDistortion,
        Self::ChromaKey,
        Self::Invert,
        Self::Grade,
    ];

    /// Returns the stable node type code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Transform => "superi.effect.transform",
            Self::Crop => "superi.effect.crop",
            Self::Opacity => "superi.effect.opacity",
            Self::Blend => "superi.effect.blend",
            Self::Composite => "superi.effect.composite",
            Self::GaussianBlur => "superi.effect.gaussian-blur",
            Self::Sharpen => "superi.effect.sharpen",
            Self::RadialDistortion => "superi.effect.radial-distortion",
            Self::ChromaKey => "superi.effect.chroma-key",
            Self::Invert => "superi.effect.invert",
            Self::Grade => "superi.effect.grade",
        }
    }

    /// Resolves one exact built-in node type code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|kind| kind.code() == code)
    }

    /// Returns the stable discovery family.
    #[must_use]
    pub const fn family(self) -> EffectNodeFamily {
        match self {
            Self::Transform | Self::Crop => EffectNodeFamily::Geometry,
            Self::Opacity | Self::Blend | Self::Composite => EffectNodeFamily::Compositing,
            Self::GaussianBlur | Self::Sharpen | Self::RadialDistortion => EffectNodeFamily::Filter,
            Self::ChromaKey => EffectNodeFamily::Keying,
            Self::Invert | Self::Grade => EffectNodeFamily::Utility,
        }
    }
}

/// Immutable complete built-in effect schema catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct EffectCatalog {
    schemas: BTreeMap<EffectNodeKind, NodeSchema>,
}

impl EffectCatalog {
    /// Builds and validates every built-in schema.
    ///
    /// # Errors
    ///
    /// Returns an internal error if a repository-owned schema literal is invalid.
    pub fn new() -> Result<Self> {
        let mut schemas = BTreeMap::new();
        for kind in EffectNodeKind::ALL {
            schemas.insert(*kind, build_definition::<()>(*kind)?.schema().clone());
        }
        Ok(Self { schemas })
    }

    /// Returns one exact schema.
    #[must_use]
    pub fn schema(&self, kind: EffectNodeKind) -> &NodeSchema {
        self.schemas
            .get(&kind)
            .expect("complete catalog contains every public kind")
    }

    /// Iterates schemas in stable catalog presentation order.
    pub fn schemas(&self) -> impl ExactSizeIterator<Item = &NodeSchema> {
        EffectNodeKind::ALL.iter().map(|kind| self.schema(*kind))
    }

    /// Registers the entire catalog in one atomic registry revision.
    ///
    /// # Errors
    ///
    /// Returns the registry conflict without partially registering the catalog.
    pub fn register(&self, registry: &mut NodeRegistry) -> Result<()> {
        registry.register_batch(self.schemas().cloned())
    }

    /// Builds one graph-native authoring definition for the caller's shared payload.
    ///
    /// # Errors
    ///
    /// Returns an internal error if a repository-owned definition literal is invalid.
    pub fn definition<T>(
        &self,
        kind: EffectNodeKind,
    ) -> Result<EffectNodeDefinition<GraphValue<T>>> {
        let definition = build_definition(kind)?;
        debug_assert_eq!(definition.schema(), self.schema(kind));
        Ok(definition)
    }

    /// Instantiates one built-in definition with caller-owned stable identities.
    ///
    /// # Errors
    ///
    /// Returns invalid input for incomplete, unknown, duplicate, cross-direction, or mistyped
    /// bindings.
    pub fn instantiate<T: Clone>(
        &self,
        kind: EffectNodeKind,
        bindings: EffectNodeBindings,
    ) -> Result<EditableNode<GraphValue<T>>> {
        self.definition(kind)?.instantiate(
            bindings,
            Vec::<(ParameterName, TypedParameterValue<GraphValue<T>>)>::new(),
        )
    }
}

#[derive(Clone, Copy)]
enum DefaultValue {
    Scalar(f64),
    Color([f64; 4]),
    Choice(&'static str),
}

#[derive(Clone, Copy)]
struct ParameterSpec {
    name: &'static str,
    value_type: &'static str,
    default: DefaultValue,
}

impl ParameterSpec {
    const fn scalar(name: &'static str, default: f64) -> Self {
        Self {
            name,
            value_type: SCALAR_VALUE,
            default: DefaultValue::Scalar(default),
        }
    }

    const fn color(name: &'static str, default: [f64; 4]) -> Self {
        Self {
            name,
            value_type: COLOR_VALUE,
            default: DefaultValue::Color(default),
        }
    }

    const fn choice(name: &'static str, default: &'static str) -> Self {
        Self {
            name,
            value_type: CHOICE_VALUE,
            default: DefaultValue::Choice(default),
        }
    }
}

fn parameter_specs(kind: EffectNodeKind) -> Vec<ParameterSpec> {
    match kind {
        EffectNodeKind::Transform => vec![
            ParameterSpec::scalar("m00", 1.0),
            ParameterSpec::scalar("m01", 0.0),
            ParameterSpec::scalar("m02", 0.0),
            ParameterSpec::scalar("m10", 0.0),
            ParameterSpec::scalar("m11", 1.0),
            ParameterSpec::scalar("m12", 0.0),
            ParameterSpec::scalar("m20", 0.0),
            ParameterSpec::scalar("m21", 0.0),
            ParameterSpec::scalar("m22", 1.0),
            ParameterSpec::choice("sampling", "bilinear"),
        ],
        EffectNodeKind::Crop => vec![
            ParameterSpec::scalar("bottom", 0.0),
            ParameterSpec::scalar("left", 0.0),
            ParameterSpec::scalar("right", 0.0),
            ParameterSpec::scalar("top", 0.0),
        ],
        EffectNodeKind::Opacity => vec![ParameterSpec::scalar("opacity", 1.0)],
        EffectNodeKind::Blend => vec![
            ParameterSpec::choice("mode", "normal"),
            ParameterSpec::scalar("opacity", 1.0),
        ],
        EffectNodeKind::Composite => vec![
            ParameterSpec::scalar("opacity", 1.0),
            ParameterSpec::choice("operator", "source-over"),
        ],
        EffectNodeKind::GaussianBlur => vec![ParameterSpec::scalar("sigma", 0.0)],
        EffectNodeKind::Sharpen => vec![
            ParameterSpec::scalar("amount", 1.0),
            ParameterSpec::scalar("sigma", 1.0),
        ],
        EffectNodeKind::RadialDistortion => vec![
            ParameterSpec::scalar("center-x", 0.5),
            ParameterSpec::scalar("center-y", 0.5),
            ParameterSpec::scalar("k1", 0.0),
            ParameterSpec::scalar("k2", 0.0),
            ParameterSpec::scalar("radius", 1.0),
        ],
        EffectNodeKind::ChromaKey => vec![
            ParameterSpec::color("key-color", [0.0, 1.0, 0.0, 1.0]),
            ParameterSpec::scalar("softness", 0.1),
            ParameterSpec::scalar("spill", 0.5),
            ParameterSpec::scalar("tolerance", 0.1),
        ],
        EffectNodeKind::Invert => vec![ParameterSpec::scalar("amount", 1.0)],
        EffectNodeKind::Grade => vec![
            ParameterSpec::scalar("gain-b", 1.0),
            ParameterSpec::scalar("gain-g", 1.0),
            ParameterSpec::scalar("gain-r", 1.0),
            ParameterSpec::scalar("offset-b", 0.0),
            ParameterSpec::scalar("offset-g", 0.0),
            ParameterSpec::scalar("offset-r", 0.0),
        ],
    }
}

fn build_definition<T>(kind: EffectNodeKind) -> Result<EffectNodeDefinition<GraphValue<T>>> {
    let image = value_type(IMAGE_VALUE)?;
    let input_names: &[&str] = match kind {
        EffectNodeKind::Blend | EffectNodeKind::Composite => &["backdrop", "source"],
        _ => &["source"],
    };
    let inputs = input_names
        .iter()
        .map(|name| {
            EffectPortDefinition::new(
                PortSchema::new(port_name(name)?, image.clone(), PortCardinality::Single),
                presentation_label(name),
                "Canonical scene-linear image input.",
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let outputs = [EffectPortDefinition::new(
        PortSchema::new(port_name("result")?, image, PortCardinality::Single),
        "Result",
        "Processed canonical scene-linear image.",
    )?];
    let parameters = parameter_specs(kind)
        .iter()
        .map(|spec| {
            let value_type = value_type(spec.value_type)?;
            EffectParameterDefinition::new(
                ParameterSchema::new(parameter_name(spec.name)?, value_type.clone(), true),
                presentation_label(spec.name),
                "Editable and animatable built-in effect state.",
                match spec.default {
                    DefaultValue::Scalar(_) => ParameterControl::Slider,
                    DefaultValue::Color(_) => ParameterControl::Color,
                    DefaultValue::Choice(_) => ParameterControl::Choice,
                },
                TypedParameterValue::new(value_type, graph_default(spec.default)?),
            )
        })
        .collect::<Result<Vec<_>>>()?;
    let roi = match kind {
        EffectNodeKind::Transform
        | EffectNodeKind::GaussianBlur
        | EffectNodeKind::Sharpen
        | EffectNodeKind::RadialDistortion => RoiBehavior::Custom,
        _ => RoiBehavior::InputBounds,
    };
    let capability = CapabilityId::new("superi.render.gpu")
        .map_err(|error| catalog_literal_error("capability", error.to_string()))?;
    EffectNodeDefinition::new(
        NodeSchemaId::new(
            NodeTypeId::new(kind.code())
                .map_err(|error| catalog_literal_error("node_type", error.to_string()))?,
            SemanticVersion::new(1, 0, 0),
        ),
        EffectMetadata::new(
            kind_label(kind),
            kind_summary(kind),
            family_label(kind.family()),
        )?,
        inputs,
        outputs,
        parameters,
        NodeBehavior::new(
            TimeBehavior::CurrentFrame,
            roi,
            ColorRequirements::Exact(ColorSpace::ACESCG),
            Determinism::Deterministic,
            CachePolicy::PerRegion,
        ),
        CapabilitySet::new([capability]),
    )
}

fn graph_default<T>(default: DefaultValue) -> Result<GraphValue<T>> {
    match default {
        DefaultValue::Scalar(value) => GraphValue::scalar(value),
        DefaultValue::Color(value) => GraphValue::color(value),
        DefaultValue::Choice(value) => GraphValue::choice(value),
    }
}

fn kind_label(kind: EffectNodeKind) -> &'static str {
    match kind {
        EffectNodeKind::Transform => "Transform",
        EffectNodeKind::Crop => "Crop",
        EffectNodeKind::Opacity => "Opacity",
        EffectNodeKind::Blend => "Blend",
        EffectNodeKind::Composite => "Composite",
        EffectNodeKind::GaussianBlur => "Gaussian Blur",
        EffectNodeKind::Sharpen => "Sharpen",
        EffectNodeKind::RadialDistortion => "Radial Distortion",
        EffectNodeKind::ChromaKey => "Chroma Key",
        EffectNodeKind::Invert => "Invert",
        EffectNodeKind::Grade => "Grade",
    }
}

fn kind_summary(kind: EffectNodeKind) -> &'static str {
    match kind {
        EffectNodeKind::Transform => "Transforms an image through an editable projective matrix.",
        EffectNodeKind::Crop => "Restricts an image through editable pixel insets.",
        EffectNodeKind::Opacity => "Scales premultiplied image opacity.",
        EffectNodeKind::Blend => "Blends a source image with a backdrop.",
        EffectNodeKind::Composite => "Composites source and backdrop images.",
        EffectNodeKind::GaussianBlur => "Applies a Gaussian neighborhood filter.",
        EffectNodeKind::Sharpen => "Sharpens an image with an unsharp mask.",
        EffectNodeKind::RadialDistortion => "Applies an editable radial lens mapping.",
        EffectNodeKind::ChromaKey => "Keys a selected color with spill suppression.",
        EffectNodeKind::Invert => "Inverts straight scene-linear color.",
        EffectNodeKind::Grade => "Applies per-channel gain and offset.",
    }
}

fn family_label(family: EffectNodeFamily) -> &'static str {
    match family {
        EffectNodeFamily::Geometry => "Geometry",
        EffectNodeFamily::Compositing => "Compositing",
        EffectNodeFamily::Filter => "Filter",
        EffectNodeFamily::Keying => "Keying",
        EffectNodeFamily::Utility => "Utility",
    }
}

fn presentation_label(value: &str) -> String {
    value
        .split('-')
        .map(|part| {
            let mut characters = part.chars();
            match characters.next() {
                Some(first) => first.to_uppercase().chain(characters).collect::<String>(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn value_type(value: &str) -> Result<ValueTypeId> {
    ValueTypeId::new(value).map_err(|error| catalog_literal_error("value_type", error.to_string()))
}

fn port_name(value: &str) -> Result<PortName> {
    PortName::new(value).map_err(|error| catalog_literal_error("port_name", error.to_string()))
}

fn parameter_name(value: &str) -> Result<ParameterName> {
    ParameterName::new(value)
        .map_err(|error| catalog_literal_error("parameter_name", error.to_string()))
}

fn catalog_literal_error(field: &'static str, detail: String) -> Error {
    Error::new(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        "repository-owned effect catalog literal is invalid",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "build")
            .with_field("field", field)
            .with_field("detail", detail),
    )
}
