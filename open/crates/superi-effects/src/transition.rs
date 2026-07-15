//! Reusable graph-native transition authoring and exact transition timing.
//!
//! This module owns visual transition schemas and their editable parameters. Timeline adjacency,
//! source and record offsets, identity, grouping, synchronization, and mutation policy remain owned
//! by `superi-timeline`; callers project that editorial state into these ordinary graph nodes.

use std::collections::BTreeMap;

use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::settings::{CapabilityId, CapabilitySet, SemanticVersion};
use superi_core::time::{Duration, RationalTime, TimeRange};
use superi_graph::mutate::{EditableNode, TypedParameterValue};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeRegistry, NodeSchema,
    NodeSchemaId, NodeTypeId, ParameterName, ParameterSchema, PortCardinality, PortName,
    PortSchema, RoiBehavior, TimeBehavior, ValueTypeId,
};
use superi_graph::value::GraphValue;

pub use crate::authoring::{
    EffectInstanceBindings as TransitionNodeBindings,
    EffectParameterBinding as TransitionParameterBinding,
};
use crate::authoring::{
    EffectMetadata, EffectNodeDefinition, EffectParameterDefinition, EffectPortDefinition,
    ParameterControl,
};

const COMPONENT: &str = "superi-effects.transition";
const IMAGE_VALUE: &str = "superi.value.image";
const SCALAR_VALUE: &str = "superi.value.scalar";
const CHOICE_VALUE: &str = "superi.value.choice";

/// Every built-in editable transition supplied by the effects crate.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum TransitionKind {
    /// A premultiplied linear interpolation from one image to the other.
    CrossDissolve,
    /// A spatially directed reveal with an editable soft edge.
    DirectionalWipe,
}

impl TransitionKind {
    /// Every transition in stable catalog presentation order.
    pub const ALL: &'static [Self] = &[Self::CrossDissolve, Self::DirectionalWipe];

    /// Returns the stable graph node type code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::CrossDissolve => "superi.transition.cross-dissolve",
            Self::DirectionalWipe => "superi.transition.directional-wipe",
        }
    }

    /// Resolves one exact built-in transition node type code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL.iter().copied().find(|kind| kind.code() == code)
    }

    const fn label(self) -> &'static str {
        match self {
            Self::CrossDissolve => "Cross Dissolve",
            Self::DirectionalWipe => "Directional Wipe",
        }
    }

    const fn summary(self) -> &'static str {
        match self {
            Self::CrossDissolve => {
                "Blends two adjacent images through editable transition progress."
            }
            Self::DirectionalWipe => {
                "Reveals the next image along an editable direction and soft edge."
            }
        }
    }
}

/// Stable directions supported by the built-in directional wipe.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum WipeDirection {
    /// Reveal from the left edge toward the right edge.
    LeftToRight,
    /// Reveal from the right edge toward the left edge.
    RightToLeft,
    /// Reveal from the top edge toward the bottom edge.
    TopToBottom,
    /// Reveal from the bottom edge toward the top edge.
    BottomToTop,
}

impl WipeDirection {
    /// Every direction in stable presentation order.
    pub const ALL: &'static [Self] = &[
        Self::LeftToRight,
        Self::RightToLeft,
        Self::TopToBottom,
        Self::BottomToTop,
    ];

    /// Returns the stable editable choice code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::LeftToRight => "left-to-right",
            Self::RightToLeft => "right-to-left",
            Self::TopToBottom => "top-to-bottom",
            Self::BottomToTop => "bottom-to-top",
        }
    }

    /// Resolves one exact editable choice code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        Self::ALL
            .iter()
            .copied()
            .find(|direction| direction.code() == code)
    }
}

/// Exact handle timing used to derive host-owned transition progress.
///
/// The range starts at `cut - from_offset`, ends at `cut + to_offset`, and is half-open. Progress
/// is clamped outside that range so visual evaluation has exact zero and one endpoint states.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TransitionTiming {
    cut: RationalTime,
    from_offset: Duration,
    to_offset: Duration,
    range: TimeRange,
}

impl TransitionTiming {
    /// Creates timing from one exact edit clock and two nonempty total handles.
    ///
    /// # Errors
    ///
    /// Returns invalid input for mixed clocks, an empty total range, or coordinate overflow.
    pub fn new(cut: RationalTime, from_offset: Duration, to_offset: Duration) -> Result<Self> {
        let clock = cut.timebase();
        if from_offset.timebase() != clock || to_offset.timebase() != clock {
            return Err(timing_error(
                "create_timing",
                "transition cut and handle offsets must use one exact edit clock",
            ));
        }
        let total = from_offset
            .value()
            .checked_add(to_offset.value())
            .ok_or_else(|| {
                timing_error(
                    "create_timing",
                    "transition handle duration exceeds the supported coordinate range",
                )
            })?;
        if total == 0 {
            return Err(timing_error(
                "create_timing",
                "transition timing requires at least one nonzero handle offset",
            ));
        }
        let from = i64::try_from(from_offset.value()).map_err(|_| {
            timing_error(
                "create_timing",
                "transition from handle exceeds the supported coordinate range",
            )
        })?;
        let to = i64::try_from(to_offset.value()).map_err(|_| {
            timing_error(
                "create_timing",
                "transition to handle exceeds the supported coordinate range",
            )
        })?;
        let start_value = cut.value().checked_sub(from).ok_or_else(|| {
            timing_error(
                "create_timing",
                "transition start exceeds the supported coordinate range",
            )
        })?;
        cut.value().checked_add(to).ok_or_else(|| {
            timing_error(
                "create_timing",
                "transition end exceeds the supported coordinate range",
            )
        })?;
        let duration = Duration::new(total, clock).map_err(|_| {
            timing_error(
                "create_timing",
                "transition duration exceeds the supported coordinate range",
            )
        })?;
        let range =
            TimeRange::new(RationalTime::new(start_value, clock), duration).map_err(|_| {
                timing_error(
                    "create_timing",
                    "transition range exceeds the supported coordinate range",
                )
            })?;
        Ok(Self {
            cut,
            from_offset,
            to_offset,
            range,
        })
    }

    /// Returns the exact editorial cut coordinate.
    #[must_use]
    pub const fn cut(self) -> RationalTime {
        self.cut
    }

    /// Returns the amount of transition time before the cut.
    #[must_use]
    pub const fn from_offset(self) -> Duration {
        self.from_offset
    }

    /// Returns the amount of transition time after the cut.
    #[must_use]
    pub const fn to_offset(self) -> Duration {
        self.to_offset
    }

    /// Returns the exact half-open transition range.
    #[must_use]
    pub const fn range(self) -> TimeRange {
        self.range
    }

    /// Maps one coordinate on the exact edit clock to closed-interval progress.
    ///
    /// # Errors
    ///
    /// Returns invalid input when the requested coordinate uses another clock.
    pub fn progress_at(self, time: RationalTime) -> Result<f64> {
        if time.timebase() != self.range.timebase() {
            return Err(timing_error(
                "progress_at",
                "transition progress requests must use the authored edit clock",
            ));
        }
        let start = self.range.start().value();
        let end = self.range.end_exclusive().map_err(|_| {
            timing_error(
                "progress_at",
                "transition end exceeds the supported coordinate range",
            )
        })?;
        if time.value() <= start {
            return Ok(0.0);
        }
        if time.value() >= end.value() {
            return Ok(1.0);
        }
        let elapsed = time.value().checked_sub(start).ok_or_else(|| {
            timing_error(
                "progress_at",
                "transition progress coordinate exceeds the supported range",
            )
        })?;
        Ok(elapsed as f64 / self.range.duration().value() as f64)
    }
}

/// Immutable complete built-in transition schema catalog.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitionCatalog {
    schemas: BTreeMap<TransitionKind, NodeSchema>,
}

impl TransitionCatalog {
    /// Builds and validates every built-in transition schema.
    ///
    /// # Errors
    ///
    /// Returns an internal error if a repository-owned schema literal is invalid.
    pub fn new() -> Result<Self> {
        let mut schemas = BTreeMap::new();
        for kind in TransitionKind::ALL {
            schemas.insert(*kind, build_definition::<()>(*kind)?.schema().clone());
        }
        Ok(Self { schemas })
    }

    /// Returns one exact transition schema.
    #[must_use]
    pub fn schema(&self, kind: TransitionKind) -> &NodeSchema {
        self.schemas
            .get(&kind)
            .expect("complete transition catalog contains every public kind")
    }

    /// Iterates schemas in stable transition presentation order.
    pub fn schemas(&self) -> impl ExactSizeIterator<Item = &NodeSchema> {
        TransitionKind::ALL.iter().map(|kind| self.schema(*kind))
    }

    /// Registers the complete transition catalog in one atomic registry revision.
    ///
    /// # Errors
    ///
    /// Returns the registry conflict without partially registering the catalog.
    pub fn register(&self, registry: &mut NodeRegistry) -> Result<()> {
        registry.register_batch(self.schemas().cloned())
    }

    /// Builds one graph-native transition authoring definition.
    ///
    /// # Errors
    ///
    /// Returns an internal error if a repository-owned definition literal is invalid.
    pub fn definition<T>(
        &self,
        kind: TransitionKind,
    ) -> Result<EffectNodeDefinition<GraphValue<T>>> {
        let definition = build_definition(kind)?;
        debug_assert_eq!(definition.schema(), self.schema(kind));
        Ok(definition)
    }

    /// Instantiates one transition with caller-owned stable identities.
    ///
    /// # Errors
    ///
    /// Returns invalid input for incomplete, unknown, duplicate, cross-direction, or mistyped
    /// bindings.
    pub fn instantiate<T: Clone>(
        &self,
        kind: TransitionKind,
        bindings: TransitionNodeBindings,
    ) -> Result<EditableNode<GraphValue<T>>> {
        self.definition(kind)?.instantiate(
            bindings,
            Vec::<(ParameterName, TypedParameterValue<GraphValue<T>>)>::new(),
        )
    }
}

fn build_definition<T>(kind: TransitionKind) -> Result<EffectNodeDefinition<GraphValue<T>>> {
    let image = value_type(IMAGE_VALUE)?;
    let inputs = [
        EffectPortDefinition::new(
            PortSchema::new(port_name("from")?, image.clone(), PortCardinality::Single),
            "From",
            "Canonical scene-linear image before the transition.",
        )?,
        EffectPortDefinition::new(
            PortSchema::new(port_name("to")?, image.clone(), PortCardinality::Single),
            "To",
            "Canonical scene-linear image after the transition.",
        )?,
    ];
    let outputs = [EffectPortDefinition::new(
        PortSchema::new(port_name("result")?, image, PortCardinality::Single),
        "Result",
        "Editable transition result in canonical scene-linear space.",
    )?];
    let mut parameters = vec![scalar_parameter(
        "progress",
        "Progress",
        "Host-owned progress from the from image to the to image.",
        0.0,
        ParameterControl::Percentage,
    )?];
    if kind == TransitionKind::DirectionalWipe {
        parameters.push(choice_parameter(
            "direction",
            "Direction",
            "Spatial direction used to reveal the to image.",
            WipeDirection::LeftToRight.code(),
        )?);
        parameters.push(scalar_parameter(
            "softness",
            "Softness",
            "Normalized width of the wipe transition band.",
            0.0,
            ParameterControl::Percentage,
        )?);
    }
    let capability = CapabilityId::new("superi.render.gpu")
        .map_err(|error| catalog_literal_error("capability", error.to_string()))?;
    EffectNodeDefinition::new(
        NodeSchemaId::new(
            NodeTypeId::new(kind.code())
                .map_err(|error| catalog_literal_error("node_type", error.to_string()))?,
            SemanticVersion::new(1, 0, 0),
        ),
        EffectMetadata::new(kind.label(), kind.summary(), "Transitions")?,
        inputs,
        outputs,
        parameters,
        NodeBehavior::new(
            TimeBehavior::CurrentFrame,
            RoiBehavior::InputBounds,
            ColorRequirements::Exact(ColorSpace::ACESCG),
            Determinism::Deterministic,
            CachePolicy::PerRegion,
        ),
        CapabilitySet::new([capability]),
    )
}

fn scalar_parameter<T>(
    name: &'static str,
    label: &'static str,
    summary: &'static str,
    default: f64,
    control: ParameterControl,
) -> Result<EffectParameterDefinition<GraphValue<T>>> {
    let value_type = value_type(SCALAR_VALUE)?;
    EffectParameterDefinition::new(
        ParameterSchema::new(parameter_name(name)?, value_type.clone(), true),
        label,
        summary,
        control,
        TypedParameterValue::new(value_type, GraphValue::scalar(default)?),
    )
}

fn choice_parameter<T>(
    name: &'static str,
    label: &'static str,
    summary: &'static str,
    default: &'static str,
) -> Result<EffectParameterDefinition<GraphValue<T>>> {
    let value_type = value_type(CHOICE_VALUE)?;
    EffectParameterDefinition::new(
        ParameterSchema::new(parameter_name(name)?, value_type.clone(), true),
        label,
        summary,
        ParameterControl::Choice,
        TypedParameterValue::new(value_type, GraphValue::choice(default)?),
    )
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

fn timing_error(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn catalog_literal_error(field: &'static str, detail: String) -> Error {
    Error::new(
        ErrorCategory::Internal,
        Recoverability::Terminal,
        "repository-owned transition catalog literal is invalid",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "build")
            .with_field("field", field)
            .with_field("detail", detail),
    )
}
