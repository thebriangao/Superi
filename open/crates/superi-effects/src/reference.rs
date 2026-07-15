//! Deterministic bounded CPU reference evaluation for built-in visual operations.
//!
//! This module is a headless oracle for schema behavior and later GPU parity. It is not the engine
//! playback or export route, and it never changes the GPU residency ownership boundary.

use std::collections::BTreeMap;
use std::mem::size_of;

use half::f16;
use serde::{Deserialize, Serialize};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::geometry::{Matrix3, PixelBounds, Rect};
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_graph::dag::GraphEdge;
use superi_graph::diagnostics::{IntrospectNode, NodeIntrospection, NodeStateFingerprint};
use superi_graph::eval::{
    EvaluateNode, EvaluationContext, EvaluationDependency, EvaluationRequest,
};
use superi_graph::ids::{NodeId, PortId};
use superi_graph::mutate::{EditableNode, GraphSnapshot};
use superi_graph::node::{NodeBehavior, NodeSchema, NodeSchemaId};
use superi_graph::value::GraphValue;
use superi_image::limits::ImageLimits;
use superi_image::ops::{crop_with_limits, transform_with_limits, ResampleFilter};
use superi_image::value::{Image, ImageDescriptor, ImageSampleType, ImageSamples};

use crate::catalog::{EffectCatalog, EffectNodeKind};
use crate::transition::{TransitionCatalog, TransitionKind, WipeDirection};

const COMPONENT: &str = "superi-effects.reference";

/// Reconstruction used by the transform reference operation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ReferenceSampling {
    /// Copy the closest source pixel.
    Nearest,
    /// Interpolate four source pixels in premultiplied space.
    Bilinear,
}

/// Separable blend functions supplied by the built-in blend node.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ReferenceBlendMode {
    /// Source color.
    Normal,
    /// Component multiplication.
    Multiply,
    /// Inverse multiplication.
    Screen,
    /// Multiply or screen according to backdrop intensity.
    Overlay,
    /// Component minimum.
    Darken,
    /// Component maximum.
    Lighten,
}

/// Porter-Duff operators supplied by the built-in composite node.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum ReferenceCompositeOperator {
    /// Source over backdrop.
    SourceOver,
    /// Backdrop over source.
    DestinationOver,
    /// Source inside backdrop.
    SourceIn,
    /// Source outside backdrop.
    SourceOut,
    /// Source atop backdrop.
    SourceAtop,
    /// Source or backdrop where only one is present.
    Xor,
}

/// Complete immutable state for one reference visual operation.
#[derive(Clone, Debug, PartialEq)]
#[non_exhaustive]
pub enum ReferenceEffectState {
    /// Premultiplied interpolation between adjacent transition images.
    CrossDissolve {
        /// Inclusive zero-through-one host-owned progress.
        progress: f64,
    },
    /// Spatial reveal between adjacent transition images.
    DirectionalWipe {
        /// Inclusive zero-through-one host-owned progress.
        progress: f64,
        /// Direction in which the to image is revealed.
        direction: WipeDirection,
        /// Inclusive zero-through-one normalized soft-edge width.
        softness: f64,
    },
    /// Forward source-to-destination projective transform.
    Transform {
        /// Row-major finite matrix.
        matrix: Matrix3,
        /// Reconstruction mode.
        sampling: ReferenceSampling,
    },
    /// Nonnegative data-window insets in pixels.
    Crop {
        /// Left inset.
        left: u32,
        /// Top inset.
        top: u32,
        /// Right inset.
        right: u32,
        /// Bottom inset.
        bottom: u32,
    },
    /// Premultiplied source opacity.
    Opacity {
        /// Inclusive zero-through-one factor.
        opacity: f64,
    },
    /// W3C separable blend followed by source-over.
    Blend {
        /// Blend function.
        mode: ReferenceBlendMode,
        /// Inclusive zero-through-one source opacity.
        opacity: f64,
    },
    /// Porter-Duff compositing.
    Composite {
        /// Composite operator.
        operator: ReferenceCompositeOperator,
        /// Inclusive zero-through-one source opacity.
        opacity: f64,
    },
    /// Gaussian blur with a three-sigma support radius.
    GaussianBlur {
        /// Nonnegative standard deviation in pixels.
        sigma: f64,
    },
    /// Unsharp-mask sharpening.
    Sharpen {
        /// Nonnegative Gaussian standard deviation in pixels.
        sigma: f64,
        /// Nonnegative difference gain.
        amount: f64,
    },
    /// Radial polynomial distortion sampled bilinearly.
    RadialDistortion {
        /// Distortion center in image coordinates.
        center: [f64; 2],
        /// Positive normalization radius in pixels.
        radius: f64,
        /// Quadratic radial coefficient.
        k1: f64,
        /// Quartic radial coefficient.
        k2: f64,
    },
    /// Straight-color chroma distance key with spill suppression.
    ChromaKey {
        /// Key color in ACEScg.
        key_color: [f64; 3],
        /// Nonnegative fully removed distance.
        tolerance: f64,
        /// Nonnegative transition width.
        softness: f64,
        /// Inclusive zero-through-one spill strength.
        spill: f64,
    },
    /// Straight-color inversion.
    Invert {
        /// Inclusive zero-through-one interpolation amount.
        amount: f64,
    },
    /// Straight-color channel gain and offset.
    Grade {
        /// Per-channel gain.
        gain: [f64; 3],
        /// Per-channel offset.
        offset: [f64; 3],
    },
}

impl ReferenceEffectState {
    /// Returns the stable operation code used in diagnostics and fingerprints.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::CrossDissolve { .. } => "cross-dissolve",
            Self::DirectionalWipe { .. } => "directional-wipe",
            Self::Transform { .. } => "transform",
            Self::Crop { .. } => "crop",
            Self::Opacity { .. } => "opacity",
            Self::Blend { .. } => "blend",
            Self::Composite { .. } => "composite",
            Self::GaussianBlur { .. } => "gaussian-blur",
            Self::Sharpen { .. } => "sharpen",
            Self::RadialDistortion { .. } => "radial-distortion",
            Self::ChromaKey { .. } => "chroma-key",
            Self::Invert { .. } => "invert",
            Self::Grade { .. } => "grade",
        }
    }

    /// Returns the exact number of semantic image inputs.
    #[must_use]
    pub const fn input_count(&self) -> usize {
        match self {
            Self::CrossDissolve { .. }
            | Self::DirectionalWipe { .. }
            | Self::Blend { .. }
            | Self::Composite { .. } => 2,
            _ => 1,
        }
    }

    const fn is_transition(&self) -> bool {
        matches!(
            self,
            Self::CrossDissolve { .. } | Self::DirectionalWipe { .. }
        )
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReferenceInput {
    From,
    To,
    Source,
    Backdrop,
}

impl ReferenceInput {
    const fn index(self) -> usize {
        match self {
            Self::From | Self::Source => 0,
            Self::To | Self::Backdrop => 1,
        }
    }

    const fn code(self) -> &'static str {
        match self {
            Self::From => "from",
            Self::To => "to",
            Self::Source => "source",
            Self::Backdrop => "backdrop",
        }
    }
}

/// One immutable executable projection of an editable built-in visual operation.
#[derive(Clone, Debug, PartialEq)]
pub struct ReferenceEffectNode {
    schema_id: NodeSchemaId,
    behavior: NodeBehavior,
    state: ReferenceEffectState,
    input_ports: BTreeMap<PortId, ReferenceInput>,
    limits: ImageLimits,
    fingerprint: NodeStateFingerprint,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ReferenceOperationKind {
    Effect(EffectNodeKind),
    Transition(TransitionKind),
}

impl ReferenceEffectNode {
    /// Returns the complete resolved typed operation state.
    #[must_use]
    pub const fn state(&self) -> &ReferenceEffectState {
        &self.state
    }

    /// Returns the finite allocation policy captured during compilation.
    #[must_use]
    pub const fn limits(&self) -> ImageLimits {
        self.limits
    }
}

impl EvaluateNode<Image> for ReferenceEffectNode {
    fn dependencies(
        &self,
        request: EvaluationRequest,
        incoming: &[GraphEdge],
    ) -> Result<Vec<EvaluationDependency>> {
        if incoming.len() != self.state.input_count() {
            return Err(reference_error(
                ErrorCategory::InvalidInput,
                "dependencies",
                "missing_effect_input",
                "visual operation must have exactly one edge for every semantic image input",
            ));
        }
        let regions = required_input_regions(&self.state, request.region())?;
        let mut seen = vec![false; self.state.input_count()];
        let mut dependencies = Vec::with_capacity(incoming.len());
        for edge in incoming {
            let semantic = self
                .input_ports
                .get(&edge.destination().port_id())
                .copied()
                .ok_or_else(|| {
                    reference_error(
                        ErrorCategory::InvalidInput,
                        "dependencies",
                        "unknown_effect_input",
                        "incoming edge targets an unknown visual operation input binding",
                    )
                })?;
            let index = semantic.index();
            if seen[index] {
                return Err(reference_error(
                    ErrorCategory::InvalidInput,
                    "dependencies",
                    "duplicate_effect_input",
                    "visual operation has more than one edge for a single-cardinality input",
                ));
            }
            seen[index] = true;
            dependencies.push(EvaluationDependency::new(
                edge.id(),
                request.frame(),
                regions[index],
            ));
        }
        if seen.iter().any(|present| !present) {
            return Err(reference_error(
                ErrorCategory::InvalidInput,
                "dependencies",
                "missing_effect_input",
                "visual operation is missing a required semantic image input",
            ));
        }
        Ok(dependencies)
    }

    fn evaluate(&self, context: &EvaluationContext<'_, Image>) -> Result<Image> {
        let mut resolved = vec![None; self.state.input_count()];
        for input in context.inputs() {
            let semantic = self
                .input_ports
                .get(&input.edge().destination().port_id())
                .copied()
                .ok_or_else(|| {
                    reference_error(
                        ErrorCategory::Internal,
                        "evaluate_node",
                        "unknown_resolved_input",
                        "resolved visual operation input has no compiled semantic binding",
                    )
                })?;
            let slot = &mut resolved[semantic.index()];
            if slot.replace(input.value().clone()).is_some() {
                return Err(reference_error(
                    ErrorCategory::Internal,
                    "evaluate_node",
                    "duplicate_resolved_input",
                    "reference evaluator received one semantic input more than once",
                ));
            }
        }
        let inputs = resolved
            .into_iter()
            .enumerate()
            .map(|(index, value)| {
                value.ok_or_else(|| {
                    reference_error(
                        ErrorCategory::Internal,
                        "evaluate_node",
                        "missing_resolved_input",
                        "reference evaluator did not receive every declared semantic input",
                    )
                    .with_context(
                        ErrorContext::new(COMPONENT, "evaluate_node")
                            .with_field("semantic_index", index.to_string()),
                    )
                })
            })
            .collect::<Result<Vec<_>>>()?;
        evaluate_reference(
            &self.state,
            &inputs,
            context.request().region(),
            &self.limits,
        )
    }
}

impl IntrospectNode for ReferenceEffectNode {
    fn introspection(&self) -> NodeIntrospection {
        NodeIntrospection::new(self.schema_id.clone(), self.behavior, self.fingerprint)
    }
}

/// Compiles one editable built-in visual operation using default image limits.
///
/// Parameter links and expressions resolve from the exact immutable graph snapshot. The runtime
/// node retains no second editable state and fingerprints every result-affecting resolved value and
/// semantic input binding.
///
/// # Errors
///
/// Returns an actionable error for an unsupported schema, missing or mistyped parameter, invalid
/// choice, invalid operation domain, or incomplete semantic port binding.
pub fn compile_reference_node<T: Clone>(
    snapshot: &GraphSnapshot<GraphValue<T>>,
    node_id: NodeId,
    node: &EditableNode<GraphValue<T>>,
) -> Result<ReferenceEffectNode> {
    compile_reference_node_with_limits(snapshot, node_id, node, ImageLimits::default())
}

/// Compiles one editable built-in visual operation using caller-selected image limits.
///
/// # Errors
///
/// Returns the same errors as [`compile_reference_node`].
pub fn compile_reference_node_with_limits<T: Clone>(
    snapshot: &GraphSnapshot<GraphValue<T>>,
    node_id: NodeId,
    node: &EditableNode<GraphValue<T>>,
    limits: ImageLimits,
) -> Result<ReferenceEffectNode> {
    let node_type = node.schema().id().node_type().as_str();
    let operation = if let Some(kind) = EffectNodeKind::from_code(node_type) {
        let catalog = EffectCatalog::new()?;
        require_exact_schema(node.schema(), catalog.schema(kind))?;
        ReferenceOperationKind::Effect(kind)
    } else if let Some(kind) = TransitionKind::from_code(node_type) {
        let catalog = TransitionCatalog::new()?;
        require_exact_schema(node.schema(), catalog.schema(kind))?;
        ReferenceOperationKind::Transition(kind)
    } else {
        return Err(reference_error(
            ErrorCategory::Unsupported,
            "compile_node",
            "unknown_effect_schema",
            "reference compiler supports only built-in effect and transition schemas",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "compile_node")
                .with_field("schema_id", node.schema().id().to_string()),
        ));
    };

    let mut parameters = BTreeMap::new();
    for parameter in node.parameters().values() {
        let evaluated = snapshot.evaluate_parameter(superi_graph::expr::ParameterAddress::new(
            node_id,
            parameter.id(),
        ))?;
        parameters.insert(
            parameter.name().as_str().to_owned(),
            evaluated.value().payload().clone(),
        );
    }
    let state = match operation {
        ReferenceOperationKind::Effect(kind) => state_from_parameters(kind, &parameters)?,
        ReferenceOperationKind::Transition(kind) => {
            transition_state_from_parameters(kind, &parameters)?
        }
    };
    validate_state(&state)?;

    let mut input_ports = BTreeMap::new();
    for (port_id, name) in node.inputs() {
        let semantic = match name.as_str() {
            "from" => ReferenceInput::From,
            "to" => ReferenceInput::To,
            "source" => ReferenceInput::Source,
            "backdrop" => ReferenceInput::Backdrop,
            _ => {
                return Err(reference_error(
                    ErrorCategory::InvalidInput,
                    "compile_node",
                    "unknown_input_binding",
                    "built-in visual operation contains an unknown semantic input",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "compile_node").with_field("input", name.as_str()),
                ));
            }
        };
        input_ports.insert(*port_id, semantic);
    }
    let mut seen = vec![false; state.input_count()];
    let bindings_are_complete = input_ports.len() == state.input_count()
        && input_ports.values().all(|semantic| {
            let index = semantic.index();
            if index >= seen.len() || seen[index] {
                false
            } else {
                seen[index] = true;
                true
            }
        })
        && seen.iter().all(|present| *present);
    if !bindings_are_complete {
        return Err(reference_error(
            ErrorCategory::InvalidInput,
            "compile_node",
            "incomplete_input_bindings",
            "built-in visual operation is missing a required semantic input binding",
        ));
    }

    let fingerprint = reference_fingerprint(node.schema().id(), &state, &input_ports);
    Ok(ReferenceEffectNode {
        schema_id: node.schema().id().clone(),
        behavior: node.schema().behavior(),
        state,
        input_ports,
        limits,
        fingerprint,
    })
}

/// Maps one output region to the conservative source work needed by the operation.
///
/// Returned regions use semantic input order. Binary effects return source then backdrop;
/// transitions return from then to. Blur and sharpen expand by the exact three-sigma integer
/// support. Transform and radial distortion return conservative inverse-mapped bounds.
///
/// # Errors
///
/// Returns invalid input for invalid state, arithmetic overflow, a singular transform, a projective
/// horizon crossing, or a non-monotonic radial mapping.
pub fn required_input_regions(
    state: &ReferenceEffectState,
    output: PixelBounds,
) -> Result<Vec<PixelBounds>> {
    validate_state(state)?;
    if output.is_empty() {
        return Err(reference_error(
            ErrorCategory::InvalidInput,
            "input_regions",
            "empty_output_region",
            "reference evaluation requires a nonempty output region",
        ));
    }
    let region = match state {
        ReferenceEffectState::Transform { matrix, sampling } => {
            let inverse = matrix.checked_inverse().map_err(|mut error| {
                error.push_context(ErrorContext::new(COMPONENT, "input_regions"));
                error
            })?;
            let mapped =
                output
                    .to_rect()
                    .checked_transform_bounds(inverse)
                    .map_err(|mut error| {
                        error.push_context(ErrorContext::new(COMPONENT, "input_regions"));
                        error
                    })?;
            let mapped = pixel_bounds_outward(mapped)?;
            match sampling {
                ReferenceSampling::Nearest => mapped,
                ReferenceSampling::Bilinear => expand_bounds(mapped, 1)?,
            }
        }
        ReferenceEffectState::GaussianBlur { sigma }
        | ReferenceEffectState::Sharpen { sigma, .. } => {
            expand_bounds(output, kernel_radius(*sigma)?)?
        }
        ReferenceEffectState::RadialDistortion {
            center,
            radius,
            k1,
            k2,
        } => radial_source_bounds(output, *center, *radius, *k1, *k2)?,
        _ => output,
    };
    Ok(vec![region; state.input_count()])
}

/// Evaluates one complete reference operation into an exact requested region.
///
/// Inputs must be premultiplied, unqualified RGBA ACEScg in binary16 or binary32. Outputs preserve
/// source color tags, channels, metadata, sample representation, and display window. RGB remains
/// extended scene-linear and is never clamped to zero through one.
///
/// # Errors
///
/// Returns invalid input or unsupported semantics for invalid state, input count, image meaning,
/// nonfinite samples, incompatible binary inputs, or an invalid spatial mapping. Returns resource
/// exhausted before output loops or allocation when the selected image limits would be exceeded.
pub fn evaluate_reference(
    state: &ReferenceEffectState,
    inputs: &[Image],
    output: PixelBounds,
    limits: &ImageLimits,
) -> Result<Image> {
    validate_state(state)?;
    if inputs.len() != state.input_count() {
        return Err(reference_error(
            ErrorCategory::InvalidInput,
            "evaluate",
            "input_count_mismatch",
            "reference operation received the wrong number of semantic inputs",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "evaluate")
                .with_field("operation", state.code())
                .with_field("expected", state.input_count().to_string())
                .with_field("actual", inputs.len().to_string()),
        ));
    }
    if output.is_empty() {
        return Err(reference_error(
            ErrorCategory::InvalidInput,
            "evaluate",
            "empty_output_region",
            "reference evaluation requires a nonempty output region",
        ));
    }
    for input in inputs {
        validate_canonical_image(input)?;
    }
    if inputs.len() == 2 {
        validate_binary_semantics(&inputs[0], &inputs[1])?;
        if state.is_transition() {
            validate_transition_semantics(&inputs[0], &inputs[1])?;
        }
    }
    ensure_output_limits(&inputs[0], output, *limits)?;

    match state {
        ReferenceEffectState::CrossDissolve { progress } => {
            evaluate_binary(&inputs[0], &inputs[1], output, *limits, |from, to| {
                Ok(transition_pixel(from, to, *progress))
            })
        }
        ReferenceEffectState::DirectionalWipe {
            progress,
            direction,
            softness,
        } => evaluate_directional_wipe(
            &inputs[0], &inputs[1], output, *progress, *direction, *softness, *limits,
        ),
        ReferenceEffectState::Transform { matrix, sampling } => transform_with_limits(
            &inputs[0],
            *matrix,
            output,
            match sampling {
                ReferenceSampling::Nearest => ResampleFilter::Nearest,
                ReferenceSampling::Bilinear => ResampleFilter::Bilinear,
            },
            limits,
        )
        .map_err(|error| with_reference_context(error, "transform")),
        ReferenceEffectState::Crop {
            left,
            top,
            right,
            bottom,
        } => evaluate_crop(&inputs[0], output, *left, *top, *right, *bottom, limits),
        ReferenceEffectState::Opacity { opacity } => {
            evaluate_pointwise(&inputs[0], output, *limits, |pixel| {
                Ok(pixel.map(|value| value * opacity))
            })
        }
        ReferenceEffectState::Blend { mode, opacity } => evaluate_binary(
            &inputs[0],
            &inputs[1],
            output,
            *limits,
            |source, backdrop| Ok(blend_pixel(source, backdrop, *mode, *opacity)),
        ),
        ReferenceEffectState::Composite { operator, opacity } => evaluate_binary(
            &inputs[0],
            &inputs[1],
            output,
            *limits,
            |source, backdrop| Ok(composite_pixel(source, backdrop, *operator, *opacity)),
        ),
        ReferenceEffectState::GaussianBlur { sigma } => {
            evaluate_blur(&inputs[0], output, *sigma, *limits)
        }
        ReferenceEffectState::Sharpen { sigma, amount } => {
            evaluate_sharpen(&inputs[0], output, *sigma, *amount, *limits)
        }
        ReferenceEffectState::RadialDistortion {
            center,
            radius,
            k1,
            k2,
        } => evaluate_distortion(&inputs[0], output, *center, *radius, *k1, *k2, *limits),
        ReferenceEffectState::ChromaKey {
            key_color,
            tolerance,
            softness,
            spill,
        } => evaluate_pointwise(&inputs[0], output, *limits, |pixel| {
            Ok(chroma_key_pixel(
                pixel, *key_color, *tolerance, *softness, *spill,
            ))
        }),
        ReferenceEffectState::Invert { amount } => {
            evaluate_pointwise(&inputs[0], output, *limits, |pixel| {
                Ok(invert_pixel(pixel, *amount))
            })
        }
        ReferenceEffectState::Grade { gain, offset } => {
            evaluate_pointwise(&inputs[0], output, *limits, |pixel| {
                Ok(grade_pixel(pixel, *gain, *offset))
            })
        }
    }
}

fn require_exact_schema(actual: &NodeSchema, expected: &NodeSchema) -> Result<()> {
    if actual == expected {
        Ok(())
    } else {
        Err(reference_error(
            ErrorCategory::Unsupported,
            "compile_node",
            "unsupported_effect_schema",
            "reference compiler requires the exact built-in visual operation schema",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "compile_node")
                .with_field("expected_schema_id", expected.id().to_string())
                .with_field("actual_schema_id", actual.id().to_string()),
        ))
    }
}

fn transition_state_from_parameters<T>(
    kind: TransitionKind,
    parameters: &BTreeMap<String, GraphValue<T>>,
) -> Result<ReferenceEffectState> {
    Ok(match kind {
        TransitionKind::CrossDissolve => ReferenceEffectState::CrossDissolve {
            progress: scalar(parameters, "progress")?,
        },
        TransitionKind::DirectionalWipe => {
            let direction_code = choice(parameters, "direction")?;
            let direction = WipeDirection::from_code(direction_code)
                .ok_or_else(|| invalid_transition_choice(kind, "direction", direction_code))?;
            ReferenceEffectState::DirectionalWipe {
                progress: scalar(parameters, "progress")?,
                direction,
                softness: scalar(parameters, "softness")?,
            }
        }
    })
}

fn state_from_parameters<T>(
    kind: EffectNodeKind,
    parameters: &BTreeMap<String, GraphValue<T>>,
) -> Result<ReferenceEffectState> {
    Ok(match kind {
        EffectNodeKind::Transform => ReferenceEffectState::Transform {
            matrix: Matrix3::from_rows([
                [
                    scalar(parameters, "m00")?,
                    scalar(parameters, "m01")?,
                    scalar(parameters, "m02")?,
                ],
                [
                    scalar(parameters, "m10")?,
                    scalar(parameters, "m11")?,
                    scalar(parameters, "m12")?,
                ],
                [
                    scalar(parameters, "m20")?,
                    scalar(parameters, "m21")?,
                    scalar(parameters, "m22")?,
                ],
            ])
            .map_err(|error| with_reference_context(error, "compile_transform"))?,
            sampling: match choice(parameters, "sampling")? {
                "nearest" => ReferenceSampling::Nearest,
                "bilinear" => ReferenceSampling::Bilinear,
                value => return Err(invalid_choice(kind, "sampling", value)),
            },
        },
        EffectNodeKind::Crop => ReferenceEffectState::Crop {
            left: integer_parameter(parameters, "left")?,
            top: integer_parameter(parameters, "top")?,
            right: integer_parameter(parameters, "right")?,
            bottom: integer_parameter(parameters, "bottom")?,
        },
        EffectNodeKind::Opacity => ReferenceEffectState::Opacity {
            opacity: scalar(parameters, "opacity")?,
        },
        EffectNodeKind::Blend => ReferenceEffectState::Blend {
            mode: match choice(parameters, "mode")? {
                "normal" => ReferenceBlendMode::Normal,
                "multiply" => ReferenceBlendMode::Multiply,
                "screen" => ReferenceBlendMode::Screen,
                "overlay" => ReferenceBlendMode::Overlay,
                "darken" => ReferenceBlendMode::Darken,
                "lighten" => ReferenceBlendMode::Lighten,
                value => return Err(invalid_choice(kind, "mode", value)),
            },
            opacity: scalar(parameters, "opacity")?,
        },
        EffectNodeKind::Composite => ReferenceEffectState::Composite {
            operator: match choice(parameters, "operator")? {
                "source-over" => ReferenceCompositeOperator::SourceOver,
                "destination-over" => ReferenceCompositeOperator::DestinationOver,
                "source-in" => ReferenceCompositeOperator::SourceIn,
                "source-out" => ReferenceCompositeOperator::SourceOut,
                "source-atop" => ReferenceCompositeOperator::SourceAtop,
                "xor" => ReferenceCompositeOperator::Xor,
                value => return Err(invalid_choice(kind, "operator", value)),
            },
            opacity: scalar(parameters, "opacity")?,
        },
        EffectNodeKind::GaussianBlur => ReferenceEffectState::GaussianBlur {
            sigma: scalar(parameters, "sigma")?,
        },
        EffectNodeKind::Sharpen => ReferenceEffectState::Sharpen {
            sigma: scalar(parameters, "sigma")?,
            amount: scalar(parameters, "amount")?,
        },
        EffectNodeKind::RadialDistortion => ReferenceEffectState::RadialDistortion {
            center: [
                scalar(parameters, "center-x")?,
                scalar(parameters, "center-y")?,
            ],
            radius: scalar(parameters, "radius")?,
            k1: scalar(parameters, "k1")?,
            k2: scalar(parameters, "k2")?,
        },
        EffectNodeKind::ChromaKey => {
            let color = color(parameters, "key-color")?;
            ReferenceEffectState::ChromaKey {
                key_color: [color[0], color[1], color[2]],
                tolerance: scalar(parameters, "tolerance")?,
                softness: scalar(parameters, "softness")?,
                spill: scalar(parameters, "spill")?,
            }
        }
        EffectNodeKind::Invert => ReferenceEffectState::Invert {
            amount: scalar(parameters, "amount")?,
        },
        EffectNodeKind::Grade => ReferenceEffectState::Grade {
            gain: [
                scalar(parameters, "gain-r")?,
                scalar(parameters, "gain-g")?,
                scalar(parameters, "gain-b")?,
            ],
            offset: [
                scalar(parameters, "offset-r")?,
                scalar(parameters, "offset-g")?,
                scalar(parameters, "offset-b")?,
            ],
        },
    })
}

fn parameter<'a, T>(
    parameters: &'a BTreeMap<String, GraphValue<T>>,
    name: &str,
) -> Result<&'a GraphValue<T>> {
    parameters.get(name).ok_or_else(|| {
        reference_error(
            ErrorCategory::InvalidInput,
            "compile_node",
            "missing_parameter",
            "built-in visual operation is missing a required parameter",
        )
        .with_context(ErrorContext::new(COMPONENT, "compile_node").with_field("parameter", name))
    })
}

fn scalar<T>(parameters: &BTreeMap<String, GraphValue<T>>, name: &str) -> Result<f64> {
    parameter(parameters, name)?.as_scalar().ok_or_else(|| {
        reference_error(
            ErrorCategory::InvalidInput,
            "compile_node",
            "mistyped_scalar_parameter",
            "built-in visual operation scalar parameter has a non-scalar payload",
        )
        .with_context(ErrorContext::new(COMPONENT, "compile_node").with_field("parameter", name))
    })
}

fn color<T>(parameters: &BTreeMap<String, GraphValue<T>>, name: &str) -> Result<[f64; 4]> {
    parameter(parameters, name)?.as_color().ok_or_else(|| {
        reference_error(
            ErrorCategory::InvalidInput,
            "compile_node",
            "mistyped_color_parameter",
            "built-in visual operation color parameter has a non-color payload",
        )
        .with_context(ErrorContext::new(COMPONENT, "compile_node").with_field("parameter", name))
    })
}

fn choice<'a, T>(parameters: &'a BTreeMap<String, GraphValue<T>>, name: &str) -> Result<&'a str> {
    parameter(parameters, name)?.as_choice().ok_or_else(|| {
        reference_error(
            ErrorCategory::InvalidInput,
            "compile_node",
            "mistyped_choice_parameter",
            "built-in visual operation choice parameter has a non-choice payload",
        )
        .with_context(ErrorContext::new(COMPONENT, "compile_node").with_field("parameter", name))
    })
}

fn integer_parameter<T>(parameters: &BTreeMap<String, GraphValue<T>>, name: &str) -> Result<u32> {
    let value = scalar(parameters, name)?;
    if value >= 0.0 && value <= f64::from(u32::MAX) && value.fract() == 0.0 {
        Ok(value as u32)
    } else {
        Err(reference_error(
            ErrorCategory::InvalidInput,
            "compile_node",
            "invalid_integer_parameter",
            "effect pixel inset parameters must be nonnegative whole numbers",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "compile_node")
                .with_field("parameter", name)
                .with_field("value", value.to_string()),
        ))
    }
}

fn invalid_choice(kind: EffectNodeKind, parameter: &str, value: &str) -> Error {
    reference_error(
        ErrorCategory::InvalidInput,
        "compile_node",
        "unknown_choice",
        "effect choice parameter contains an unsupported value",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "compile_node")
            .with_field("node_type", kind.code())
            .with_field("parameter", parameter)
            .with_field("value", value),
    )
}

fn invalid_transition_choice(kind: TransitionKind, parameter: &str, value: &str) -> Error {
    reference_error(
        ErrorCategory::InvalidInput,
        "compile_node",
        "unknown_choice",
        "transition choice parameter contains an unsupported value",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "compile_node")
            .with_field("node_type", kind.code())
            .with_field("parameter", parameter)
            .with_field("value", value),
    )
}

fn reference_fingerprint(
    schema_id: &NodeSchemaId,
    state: &ReferenceEffectState,
    input_ports: &BTreeMap<PortId, ReferenceInput>,
) -> NodeStateFingerprint {
    let mut bytes = Vec::new();
    push_text(&mut bytes, &schema_id.to_string());
    push_state(&mut bytes, state);
    push_u64(&mut bytes, input_ports.len() as u64);
    for (port_id, semantic) in input_ports {
        bytes.extend_from_slice(&port_id.to_bytes());
        push_text(&mut bytes, semantic.code());
    }
    NodeStateFingerprint::from_canonical_bytes(bytes)
}

fn push_state(bytes: &mut Vec<u8>, state: &ReferenceEffectState) {
    push_text(bytes, state.code());
    match state {
        ReferenceEffectState::CrossDissolve { progress } => push_f64(bytes, *progress),
        ReferenceEffectState::DirectionalWipe {
            progress,
            direction,
            softness,
        } => {
            push_f64(bytes, *progress);
            push_text(bytes, direction.code());
            push_f64(bytes, *softness);
        }
        ReferenceEffectState::Transform { matrix, sampling } => {
            for row in matrix.rows() {
                for value in row {
                    push_f64(bytes, value);
                }
            }
            push_text(
                bytes,
                match sampling {
                    ReferenceSampling::Nearest => "nearest",
                    ReferenceSampling::Bilinear => "bilinear",
                },
            );
        }
        ReferenceEffectState::Crop {
            left,
            top,
            right,
            bottom,
        } => {
            for value in [left, top, right, bottom] {
                bytes.extend_from_slice(&value.to_be_bytes());
            }
        }
        ReferenceEffectState::Opacity { opacity }
        | ReferenceEffectState::Invert { amount: opacity } => push_f64(bytes, *opacity),
        ReferenceEffectState::Blend { mode, opacity } => {
            push_text(
                bytes,
                match mode {
                    ReferenceBlendMode::Normal => "normal",
                    ReferenceBlendMode::Multiply => "multiply",
                    ReferenceBlendMode::Screen => "screen",
                    ReferenceBlendMode::Overlay => "overlay",
                    ReferenceBlendMode::Darken => "darken",
                    ReferenceBlendMode::Lighten => "lighten",
                },
            );
            push_f64(bytes, *opacity);
        }
        ReferenceEffectState::Composite { operator, opacity } => {
            push_text(
                bytes,
                match operator {
                    ReferenceCompositeOperator::SourceOver => "source-over",
                    ReferenceCompositeOperator::DestinationOver => "destination-over",
                    ReferenceCompositeOperator::SourceIn => "source-in",
                    ReferenceCompositeOperator::SourceOut => "source-out",
                    ReferenceCompositeOperator::SourceAtop => "source-atop",
                    ReferenceCompositeOperator::Xor => "xor",
                },
            );
            push_f64(bytes, *opacity);
        }
        ReferenceEffectState::GaussianBlur { sigma } => push_f64(bytes, *sigma),
        ReferenceEffectState::Sharpen { sigma, amount } => {
            push_f64(bytes, *sigma);
            push_f64(bytes, *amount);
        }
        ReferenceEffectState::RadialDistortion {
            center,
            radius,
            k1,
            k2,
        } => {
            for value in [center[0], center[1], *radius, *k1, *k2] {
                push_f64(bytes, value);
            }
        }
        ReferenceEffectState::ChromaKey {
            key_color,
            tolerance,
            softness,
            spill,
        } => {
            for value in [
                key_color[0],
                key_color[1],
                key_color[2],
                *tolerance,
                *softness,
                *spill,
            ] {
                push_f64(bytes, value);
            }
        }
        ReferenceEffectState::Grade { gain, offset } => {
            for value in gain.iter().chain(offset).copied() {
                push_f64(bytes, value);
            }
        }
    }
}

fn push_f64(bytes: &mut Vec<u8>, value: f64) {
    bytes.extend_from_slice(&value.to_bits().to_be_bytes());
}

fn push_text(bytes: &mut Vec<u8>, value: &str) {
    push_u64(bytes, value.len() as u64);
    bytes.extend_from_slice(value.as_bytes());
}

fn push_u64(bytes: &mut Vec<u8>, value: u64) {
    bytes.extend_from_slice(&value.to_be_bytes());
}

fn validate_state(state: &ReferenceEffectState) -> Result<()> {
    let finite = |value: f64| value.is_finite();
    let unit = |value: f64| finite(value) && (0.0..=1.0).contains(&value);
    let valid = match state {
        ReferenceEffectState::CrossDissolve { progress } => unit(*progress),
        ReferenceEffectState::DirectionalWipe {
            progress, softness, ..
        } => unit(*progress) && unit(*softness),
        ReferenceEffectState::Transform { .. } | ReferenceEffectState::Crop { .. } => true,
        ReferenceEffectState::Opacity { opacity }
        | ReferenceEffectState::Blend { opacity, .. }
        | ReferenceEffectState::Composite { opacity, .. }
        | ReferenceEffectState::Invert { amount: opacity } => unit(*opacity),
        ReferenceEffectState::GaussianBlur { sigma } => finite(*sigma) && *sigma >= 0.0,
        ReferenceEffectState::Sharpen { sigma, amount } => {
            finite(*sigma) && *sigma >= 0.0 && finite(*amount) && *amount >= 0.0
        }
        ReferenceEffectState::RadialDistortion {
            center,
            radius,
            k1,
            k2,
        } => {
            center.iter().copied().all(finite)
                && finite(*radius)
                && *radius > 0.0
                && finite(*k1)
                && finite(*k2)
        }
        ReferenceEffectState::ChromaKey {
            key_color,
            tolerance,
            softness,
            spill,
        } => {
            key_color.iter().copied().all(finite)
                && finite(*tolerance)
                && *tolerance >= 0.0
                && finite(*softness)
                && *softness >= 0.0
                && unit(*spill)
        }
        ReferenceEffectState::Grade { gain, offset } => {
            gain.iter().chain(offset).copied().all(finite)
        }
    };
    if valid {
        Ok(())
    } else {
        Err(reference_error(
            ErrorCategory::InvalidInput,
            "validate_state",
            "invalid_operation_state",
            "reference operation state is outside its finite supported domain",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "validate_state").with_field("operation", state.code()),
        ))
    }
}

fn validate_canonical_image(image: &Image) -> Result<()> {
    let descriptor = image.descriptor();
    let format_supported = matches!(
        descriptor.pixel_format(),
        PixelFormat::Rgba16Float | PixelFormat::Rgba32Float
    );
    let channels = descriptor
        .channels()
        .iter()
        .map(|channel| channel.as_str())
        .collect::<Vec<_>>();
    if !format_supported
        || descriptor.color_space() != superi_core::color_space::ColorSpace::ACESCG
        || descriptor.alpha_mode() != AlphaMode::Premultiplied
        || channels != ["R", "G", "B", "A"]
    {
        return Err(reference_error(
            ErrorCategory::Unsupported,
            "validate_image",
            "unsupported_image_semantics",
            "reference operations require premultiplied unqualified RGBA ACEScg binary16 or binary32",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "validate_image")
                .with_field("pixel_format", descriptor.pixel_format().code())
                .with_field("color_space", format!("{:?}", descriptor.color_space()))
                .with_field("alpha_mode", descriptor.alpha_mode().code()),
        ));
    }
    for index in 0..image.samples().len() {
        let value = image
            .samples()
            .float_value(index)
            .expect("validated reference image has floating samples");
        if !value.is_finite() {
            return Err(reference_error(
                ErrorCategory::InvalidInput,
                "validate_image",
                "nonfinite_image_sample",
                "reference operations reject nonfinite image samples",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "validate_image")
                    .with_field("sample_index", index.to_string()),
            ));
        }
        if index % 4 == 3 && !(0.0..=1.0).contains(&value) {
            return Err(reference_error(
                ErrorCategory::InvalidInput,
                "validate_image",
                "invalid_alpha_sample",
                "reference operation alpha samples must remain between zero and one",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "validate_image")
                    .with_field("sample_index", index.to_string()),
            ));
        }
    }
    Ok(())
}

fn validate_binary_semantics(source: &Image, backdrop: &Image) -> Result<()> {
    let source = source.descriptor();
    let backdrop = backdrop.descriptor();
    if source.pixel_format() != backdrop.pixel_format()
        || source.color_tags() != backdrop.color_tags()
        || source.alpha_mode() != backdrop.alpha_mode()
        || source.channels() != backdrop.channels()
    {
        return Err(reference_error(
            ErrorCategory::InvalidInput,
            "validate_binary",
            "incompatible_image_semantics",
            "binary reference inputs must share pixel, channel, color, and alpha semantics",
        ));
    }
    Ok(())
}

fn validate_transition_semantics(from: &Image, to: &Image) -> Result<()> {
    if from.descriptor().display_window() != to.descriptor().display_window() {
        return Err(reference_error(
            ErrorCategory::InvalidInput,
            "validate_transition",
            "incompatible_transition_windows",
            "transition inputs must share one canonical display window",
        ));
    }
    Ok(())
}

fn evaluate_crop(
    source: &Image,
    output: PixelBounds,
    left: u32,
    top: u32,
    right: u32,
    bottom: u32,
    limits: &ImageLimits,
) -> Result<Image> {
    let source_bounds = source.descriptor().data_window();
    let left = i32::try_from(left).map_err(|_| crop_inset_error())?;
    let top = i32::try_from(top).map_err(|_| crop_inset_error())?;
    let right = i32::try_from(right).map_err(|_| crop_inset_error())?;
    let bottom = i32::try_from(bottom).map_err(|_| crop_inset_error())?;
    let min_x = source_bounds
        .min_x()
        .checked_add(left)
        .ok_or_else(crop_inset_error)?;
    let min_y = source_bounds
        .min_y()
        .checked_add(top)
        .ok_or_else(crop_inset_error)?;
    let max_x = source_bounds
        .max_x()
        .checked_sub(right)
        .ok_or_else(crop_inset_error)?;
    let max_y = source_bounds
        .max_y()
        .checked_sub(bottom)
        .ok_or_else(crop_inset_error)?;
    if min_x >= max_x || min_y >= max_y {
        return Err(reference_error(
            ErrorCategory::InvalidInput,
            "crop",
            "empty_crop",
            "effect crop insets remove the complete source data window",
        ));
    }
    let authored = PixelBounds::new(min_x, min_y, max_x, max_y)
        .map_err(|error| with_reference_context(error, "crop"))?;
    let cropped = crop_with_limits(source, authored, limits)
        .map_err(|error| with_reference_context(error, "crop"))?;
    crop_with_limits(&cropped, output, limits)
        .map_err(|error| with_reference_context(error, "crop"))
}

fn crop_inset_error() -> Error {
    reference_error(
        ErrorCategory::InvalidInput,
        "crop",
        "crop_inset_overflow",
        "effect crop insets exceed the supported coordinate range",
    )
}

fn evaluate_pointwise(
    source: &Image,
    output: PixelBounds,
    limits: ImageLimits,
    mut operation: impl FnMut([f64; 4]) -> Result<[f64; 4]>,
) -> Result<Image> {
    let mut result = pixel_buffer(output, limits)?;
    for y in output.min_y()..output.max_y() {
        for x in output.min_x()..output.max_x() {
            let pixel = operation(read_pixel(source, x, y))?;
            validate_output_pixel(pixel)?;
            result.push(pixel);
        }
    }
    build_output(source, output, result, limits)
}

fn evaluate_binary(
    source: &Image,
    backdrop: &Image,
    output: PixelBounds,
    limits: ImageLimits,
    mut operation: impl FnMut([f64; 4], [f64; 4]) -> Result<[f64; 4]>,
) -> Result<Image> {
    let mut result = pixel_buffer(output, limits)?;
    for y in output.min_y()..output.max_y() {
        for x in output.min_x()..output.max_x() {
            let pixel = operation(read_pixel(source, x, y), read_pixel(backdrop, x, y))?;
            validate_output_pixel(pixel)?;
            result.push(pixel);
        }
    }
    build_output(source, output, result, limits)
}

fn evaluate_directional_wipe(
    from: &Image,
    to: &Image,
    output: PixelBounds,
    progress: f64,
    direction: WipeDirection,
    softness: f64,
    limits: ImageLimits,
) -> Result<Image> {
    let display = from.descriptor().display_window();
    let width = f64::from(display.width());
    let height = f64::from(display.height());
    let mut result = pixel_buffer(output, limits)?;
    for y in output.min_y()..output.max_y() {
        for x in output.min_x()..output.max_x() {
            let horizontal = (f64::from(x) - f64::from(display.min_x()) + 0.5) / width;
            let vertical = (f64::from(y) - f64::from(display.min_y()) + 0.5) / height;
            let coordinate = match direction {
                WipeDirection::LeftToRight => horizontal,
                WipeDirection::RightToLeft => 1.0 - horizontal,
                WipeDirection::TopToBottom => vertical,
                WipeDirection::BottomToTop => 1.0 - vertical,
            };
            let to_weight = wipe_weight(coordinate, progress, softness);
            let pixel = transition_pixel(read_pixel(from, x, y), read_pixel(to, x, y), to_weight);
            validate_output_pixel(pixel)?;
            result.push(pixel);
        }
    }
    build_output(from, output, result, limits)
}

fn wipe_weight(coordinate: f64, progress: f64, softness: f64) -> f64 {
    if progress <= 0.0 {
        return 0.0;
    }
    if progress >= 1.0 {
        return 1.0;
    }
    if softness == 0.0 {
        return if coordinate <= progress { 1.0 } else { 0.0 };
    }
    let start = progress - softness * 0.5;
    let end = progress + softness * 0.5;
    let normalized = ((coordinate - start) / (end - start)).clamp(0.0, 1.0);
    let smooth = normalized * normalized * (3.0 - 2.0 * normalized);
    1.0 - smooth
}

fn transition_pixel(from: [f64; 4], to: [f64; 4], progress: f64) -> [f64; 4] {
    std::array::from_fn(|channel| from[channel] * (1.0 - progress) + to[channel] * progress)
}

fn evaluate_blur(
    source: &Image,
    output: PixelBounds,
    sigma: f64,
    limits: ImageLimits,
) -> Result<Image> {
    if sigma == 0.0 {
        return crop_with_limits(source, output, &limits)
            .map_err(|error| with_reference_context(error, "gaussian_blur"));
    }
    let weights = gaussian_weights(sigma, limits)?;
    let radius = i32::try_from(weights.len() / 2).map_err(|_| kernel_size_error())?;
    let mut result = pixel_buffer(output, limits)?;
    for y in output.min_y()..output.max_y() {
        for x in output.min_x()..output.max_x() {
            let mut pixel = [0.0; 4];
            for (weight_y_index, weight_y) in weights.iter().copied().enumerate() {
                let offset_y =
                    i32::try_from(weight_y_index).map_err(|_| kernel_size_error())? - radius;
                for (weight_x_index, weight_x) in weights.iter().copied().enumerate() {
                    let offset_x =
                        i32::try_from(weight_x_index).map_err(|_| kernel_size_error())? - radius;
                    let sample = match (x.checked_add(offset_x), y.checked_add(offset_y)) {
                        (Some(sample_x), Some(sample_y)) => read_pixel(source, sample_x, sample_y),
                        _ => [0.0; 4],
                    };
                    let weight = weight_x * weight_y;
                    for channel in 0..4 {
                        pixel[channel] += sample[channel] * weight;
                    }
                }
            }
            validate_output_pixel(pixel)?;
            result.push(pixel);
        }
    }
    build_output(source, output, result, limits)
}

fn evaluate_sharpen(
    source: &Image,
    output: PixelBounds,
    sigma: f64,
    amount: f64,
    limits: ImageLimits,
) -> Result<Image> {
    let blurred = evaluate_blur(source, output, sigma, limits)?;
    let mut result = pixel_buffer(output, limits)?;
    for y in output.min_y()..output.max_y() {
        for x in output.min_x()..output.max_x() {
            let source_pixel = read_pixel(source, x, y);
            let blurred_pixel = read_pixel(&blurred, x, y);
            let mut pixel = source_pixel;
            for channel in 0..3 {
                pixel[channel] = source_pixel[channel]
                    + amount * (source_pixel[channel] - blurred_pixel[channel]);
            }
            validate_output_pixel(pixel)?;
            result.push(pixel);
        }
    }
    build_output(source, output, result, limits)
}

fn evaluate_distortion(
    source: &Image,
    output: PixelBounds,
    center: [f64; 2],
    radius: f64,
    k1: f64,
    k2: f64,
    limits: ImageLimits,
) -> Result<Image> {
    let _ = radial_source_bounds(output, center, radius, k1, k2)?;
    let mut result = pixel_buffer(output, limits)?;
    for y in output.min_y()..output.max_y() {
        for x in output.min_x()..output.max_x() {
            let output_x = f64::from(x) + 0.5;
            let output_y = f64::from(y) + 0.5;
            let normalized_x = (output_x - center[0]) / radius;
            let normalized_y = (output_y - center[1]) / radius;
            let radial_squared = normalized_x * normalized_x + normalized_y * normalized_y;
            let factor = 1.0 + k1 * radial_squared + k2 * radial_squared * radial_squared;
            let source_x = center[0] + normalized_x * factor * radius;
            let source_y = center[1] + normalized_y * factor * radius;
            let pixel = sample_bilinear(source, source_x, source_y);
            validate_output_pixel(pixel)?;
            result.push(pixel);
        }
    }
    build_output(source, output, result, limits)
}

fn blend_pixel(
    source: [f64; 4],
    backdrop: [f64; 4],
    mode: ReferenceBlendMode,
    opacity: f64,
) -> [f64; 4] {
    let source_alpha = source[3] * opacity;
    let backdrop_alpha = backdrop[3];
    let source_straight = straight_rgb(source);
    let backdrop_straight = straight_rgb(backdrop);
    let mut result = [0.0; 4];
    for channel in 0..3 {
        let blended = blend_component(backdrop_straight[channel], source_straight[channel], mode);
        result[channel] = (1.0 - source_alpha) * backdrop[channel]
            + (1.0 - backdrop_alpha) * source[channel] * opacity
            + source_alpha * backdrop_alpha * blended;
    }
    result[3] = source_alpha + backdrop_alpha * (1.0 - source_alpha);
    result
}

fn blend_component(backdrop: f64, source: f64, mode: ReferenceBlendMode) -> f64 {
    match mode {
        ReferenceBlendMode::Normal => source,
        ReferenceBlendMode::Multiply => backdrop * source,
        ReferenceBlendMode::Screen => backdrop + source - backdrop * source,
        ReferenceBlendMode::Overlay => {
            if backdrop <= 0.5 {
                2.0 * backdrop * source
            } else {
                1.0 - 2.0 * (1.0 - backdrop) * (1.0 - source)
            }
        }
        ReferenceBlendMode::Darken => backdrop.min(source),
        ReferenceBlendMode::Lighten => backdrop.max(source),
    }
}

fn composite_pixel(
    source: [f64; 4],
    backdrop: [f64; 4],
    operator: ReferenceCompositeOperator,
    opacity: f64,
) -> [f64; 4] {
    let source = source.map(|value| value * opacity);
    let source_alpha = source[3];
    let backdrop_alpha = backdrop[3];
    let (source_factor, backdrop_factor) = match operator {
        ReferenceCompositeOperator::SourceOver => (1.0, 1.0 - source_alpha),
        ReferenceCompositeOperator::DestinationOver => (1.0 - backdrop_alpha, 1.0),
        ReferenceCompositeOperator::SourceIn => (backdrop_alpha, 0.0),
        ReferenceCompositeOperator::SourceOut => (1.0 - backdrop_alpha, 0.0),
        ReferenceCompositeOperator::SourceAtop => (backdrop_alpha, 1.0 - source_alpha),
        ReferenceCompositeOperator::Xor => (1.0 - backdrop_alpha, 1.0 - source_alpha),
    };
    let mut result = [0.0; 4];
    for channel in 0..4 {
        result[channel] = source[channel] * source_factor + backdrop[channel] * backdrop_factor;
    }
    result
}

fn chroma_key_pixel(
    pixel: [f64; 4],
    key_color: [f64; 3],
    tolerance: f64,
    softness: f64,
    spill: f64,
) -> [f64; 4] {
    let straight = straight_rgb(pixel);
    let distance = ((straight[0] - key_color[0]).powi(2)
        + (straight[1] - key_color[1]).powi(2)
        + (straight[2] - key_color[2]).powi(2))
    .sqrt();
    let keep = if softness == 0.0 {
        if distance <= tolerance {
            0.0
        } else {
            1.0
        }
    } else {
        let amount = ((distance - tolerance) / softness).clamp(0.0, 1.0);
        amount * amount * (3.0 - 2.0 * amount)
    };
    let key_length = (key_color[0].powi(2) + key_color[1].powi(2) + key_color[2].powi(2)).sqrt();
    let mut corrected = straight;
    if key_length > 0.0 {
        let direction = key_color.map(|component| component / key_length);
        let projection = corrected
            .iter()
            .zip(direction)
            .map(|(component, direction)| component * direction)
            .sum::<f64>()
            .max(0.0);
        let reduction = (1.0 - keep) * spill * projection;
        for channel in 0..3 {
            corrected[channel] -= direction[channel] * reduction;
        }
    }
    let alpha = pixel[3] * keep;
    [
        corrected[0] * alpha,
        corrected[1] * alpha,
        corrected[2] * alpha,
        alpha,
    ]
}

fn invert_pixel(pixel: [f64; 4], amount: f64) -> [f64; 4] {
    let straight = straight_rgb(pixel);
    let alpha = pixel[3];
    let inverted = straight.map(|value| value * (1.0 - amount) + (1.0 - value) * amount);
    [
        inverted[0] * alpha,
        inverted[1] * alpha,
        inverted[2] * alpha,
        alpha,
    ]
}

fn grade_pixel(pixel: [f64; 4], gain: [f64; 3], offset: [f64; 3]) -> [f64; 4] {
    let straight = straight_rgb(pixel);
    let alpha = pixel[3];
    [
        (straight[0] * gain[0] + offset[0]) * alpha,
        (straight[1] * gain[1] + offset[1]) * alpha,
        (straight[2] * gain[2] + offset[2]) * alpha,
        alpha,
    ]
}

fn straight_rgb(pixel: [f64; 4]) -> [f64; 3] {
    if pixel[3] == 0.0 {
        [0.0; 3]
    } else {
        [
            pixel[0] / pixel[3],
            pixel[1] / pixel[3],
            pixel[2] / pixel[3],
        ]
    }
}

fn gaussian_weights(sigma: f64, limits: ImageLimits) -> Result<Vec<f64>> {
    let radius = kernel_radius(sigma)?;
    let radius = i32::try_from(radius).map_err(|_| kernel_size_error())?;
    let length = usize::try_from(i64::from(radius) * 2 + 1).map_err(|_| kernel_size_error())?;
    ensure_working_allocation::<f64>(length, limits, "gaussian_kernel")?;
    let mut weights = Vec::new();
    weights
        .try_reserve_exact(length)
        .map_err(|_| kernel_size_error())?;
    let denominator = 2.0 * sigma * sigma;
    let mut total = 0.0;
    for offset in -radius..=radius {
        let offset = f64::from(offset);
        let weight = (-(offset * offset) / denominator).exp();
        weights.push(weight);
        total += weight;
    }
    if !total.is_finite() || total <= 0.0 {
        return Err(kernel_size_error());
    }
    for weight in &mut weights {
        *weight /= total;
    }
    Ok(weights)
}

fn kernel_radius(sigma: f64) -> Result<u32> {
    if sigma == 0.0 {
        return Ok(0);
    }
    let radius = (sigma * 3.0).ceil();
    if !radius.is_finite() || radius > f64::from(i32::MAX) {
        Err(kernel_size_error())
    } else {
        Ok(radius as u32)
    }
}

fn kernel_size_error() -> Error {
    reference_error(
        ErrorCategory::ResourceExhausted,
        "gaussian_kernel",
        "kernel_too_large",
        "reference Gaussian kernel exceeds the supported bounded size",
    )
}

fn read_pixel(image: &Image, x: i32, y: i32) -> [f64; 4] {
    let bounds = image.descriptor().data_window();
    if !bounds.contains(x, y) {
        return [0.0; 4];
    }
    let row = i64::from(y) - i64::from(bounds.min_y());
    let column = i64::from(x) - i64::from(bounds.min_x());
    let width = i64::from(bounds.width());
    let pixel = usize::try_from(row * width + column)
        .expect("validated image coordinates fit the source sample allocation");
    let base = pixel * 4;
    [
        f64::from(image.samples().float_value(base).expect("floating image")),
        f64::from(
            image
                .samples()
                .float_value(base + 1)
                .expect("floating image"),
        ),
        f64::from(
            image
                .samples()
                .float_value(base + 2)
                .expect("floating image"),
        ),
        f64::from(
            image
                .samples()
                .float_value(base + 3)
                .expect("floating image"),
        ),
    ]
}

fn sample_bilinear(image: &Image, center_x: f64, center_y: f64) -> [f64; 4] {
    let lattice_x = center_x - 0.5;
    let lattice_y = center_y - 0.5;
    if !lattice_x.is_finite() || !lattice_y.is_finite() {
        return [0.0; 4];
    }
    let floor_x = lattice_x.floor();
    let floor_y = lattice_y.floor();
    if floor_x < f64::from(i32::MIN)
        || floor_x > f64::from(i32::MAX - 1)
        || floor_y < f64::from(i32::MIN)
        || floor_y > f64::from(i32::MAX - 1)
    {
        return [0.0; 4];
    }
    let base_x = floor_x as i32;
    let base_y = floor_y as i32;
    let fraction_x = lattice_x - floor_x;
    let fraction_y = lattice_y - floor_y;
    let mut result = [0.0; 4];
    for (offset_y, weight_y) in [(0_i32, 1.0 - fraction_y), (1, fraction_y)] {
        for (offset_x, weight_x) in [(0_i32, 1.0 - fraction_x), (1, fraction_x)] {
            let sample = read_pixel(image, base_x + offset_x, base_y + offset_y);
            let weight = weight_x * weight_y;
            for channel in 0..4 {
                result[channel] += sample[channel] * weight;
            }
        }
    }
    result
}

fn build_output(
    source: &Image,
    output: PixelBounds,
    pixels: Vec<[f64; 4]>,
    limits: ImageLimits,
) -> Result<Image> {
    ensure_output_limits(source, output, limits)?;
    if pixels.len() != pixel_capacity(output)? {
        return Err(reference_error(
            ErrorCategory::Internal,
            "build_output",
            "pixel_count_mismatch",
            "reference evaluator produced an invalid pixel count",
        ));
    }
    let descriptor = ImageDescriptor::new_with_color_tags(
        output,
        source.descriptor().display_window(),
        source.descriptor().pixel_format(),
        source.descriptor().color_tags().clone(),
        source.descriptor().alpha_mode(),
    )
    .and_then(|descriptor| descriptor.with_channels(source.descriptor().channels().clone()))
    .map_err(|error| with_reference_context(error, "build_output"))?;
    let sample_count = pixels
        .len()
        .checked_mul(4)
        .ok_or_else(allocation_overflow_error)?;
    let samples = match descriptor.sample_type() {
        ImageSampleType::F16 => {
            let mut values = Vec::new();
            values
                .try_reserve_exact(sample_count)
                .map_err(|_| allocation_error())?;
            for pixel in pixels {
                for value in pixel {
                    let value = value as f32;
                    if !value.is_finite() {
                        return Err(output_range_error());
                    }
                    let value = f16::from_f32(value);
                    if !value.is_finite() {
                        return Err(output_range_error());
                    }
                    values.push(value.to_bits());
                }
            }
            ImageSamples::from_f16_bits(values)
        }
        ImageSampleType::F32 => {
            let mut values = Vec::new();
            values
                .try_reserve_exact(sample_count)
                .map_err(|_| allocation_error())?;
            for pixel in pixels {
                for value in pixel {
                    let value = value as f32;
                    if !value.is_finite() {
                        return Err(output_range_error());
                    }
                    values.push(value);
                }
            }
            ImageSamples::from_f32(values)
        }
        _ => unreachable!("canonical reference outputs are floating RGBA"),
    };
    Image::new_with_metadata(descriptor, samples, source.metadata().clone())
        .map_err(|error| with_reference_context(error, "build_output"))
}

fn validate_output_pixel(pixel: [f64; 4]) -> Result<()> {
    if pixel.iter().copied().all(f64::is_finite) && (0.0..=1.0).contains(&pixel[3]) {
        Ok(())
    } else {
        Err(output_range_error())
    }
}

fn ensure_output_limits(source: &Image, output: PixelBounds, limits: ImageLimits) -> Result<()> {
    if output.width() > limits.max_width() || output.height() > limits.max_height() {
        return Err(reference_error(
            ErrorCategory::ResourceExhausted,
            "limits",
            "dimension_limit",
            "reference output dimensions exceed the configured image limits",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "limits")
                .with_field("width", output.width().to_string())
                .with_field("height", output.height().to_string())
                .with_field("max_width", limits.max_width().to_string())
                .with_field("max_height", limits.max_height().to_string()),
        ));
    }
    if source.descriptor().channels().len() > limits.max_channels() {
        return Err(reference_error(
            ErrorCategory::ResourceExhausted,
            "limits",
            "channel_limit",
            "reference output channels exceed the configured image limits",
        ));
    }
    let samples = u64::from(output.width())
        .checked_mul(u64::from(output.height()))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or_else(allocation_overflow_error)?;
    let sample_bytes = u64::from(source.descriptor().sample_type().bits() / 8);
    let bytes = samples
        .checked_mul(sample_bytes)
        .ok_or_else(allocation_overflow_error)?;
    if bytes > limits.max_memory_bytes() {
        return Err(reference_error(
            ErrorCategory::ResourceExhausted,
            "limits",
            "memory_limit",
            "reference output allocation exceeds the configured image limits",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "limits")
                .with_field("allocation_bytes", bytes.to_string())
                .with_field("max_memory_bytes", limits.max_memory_bytes().to_string()),
        ));
    }
    Ok(())
}

fn pixel_capacity(bounds: PixelBounds) -> Result<usize> {
    usize::try_from(u64::from(bounds.width()) * u64::from(bounds.height()))
        .map_err(|_| allocation_overflow_error())
}

fn pixel_buffer(bounds: PixelBounds, limits: ImageLimits) -> Result<Vec<[f64; 4]>> {
    let capacity = pixel_capacity(bounds)?;
    ensure_working_allocation::<[f64; 4]>(capacity, limits, "pixel_buffer")?;
    let mut values = Vec::new();
    values
        .try_reserve_exact(capacity)
        .map_err(|_| allocation_error())?;
    Ok(values)
}

fn ensure_working_allocation<T>(
    elements: usize,
    limits: ImageLimits,
    allocation: &'static str,
) -> Result<()> {
    let bytes = elements
        .checked_mul(size_of::<T>())
        .and_then(|bytes| u64::try_from(bytes).ok())
        .ok_or_else(allocation_overflow_error)?;
    if bytes > limits.max_memory_bytes() {
        Err(reference_error(
            ErrorCategory::ResourceExhausted,
            "limits",
            "working_memory_limit",
            "reference working allocation exceeds the configured image limits",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "limits")
                .with_field("allocation", allocation)
                .with_field("allocation_bytes", bytes.to_string())
                .with_field("max_memory_bytes", limits.max_memory_bytes().to_string()),
        ))
    } else {
        Ok(())
    }
}

fn expand_bounds(bounds: PixelBounds, pixels: u32) -> Result<PixelBounds> {
    let pixels = i32::try_from(pixels).map_err(|_| bounds_overflow_error())?;
    PixelBounds::new(
        bounds
            .min_x()
            .checked_sub(pixels)
            .ok_or_else(bounds_overflow_error)?,
        bounds
            .min_y()
            .checked_sub(pixels)
            .ok_or_else(bounds_overflow_error)?,
        bounds
            .max_x()
            .checked_add(pixels)
            .ok_or_else(bounds_overflow_error)?,
        bounds
            .max_y()
            .checked_add(pixels)
            .ok_or_else(bounds_overflow_error)?,
    )
    .map_err(|error| with_reference_context(error, "expand_bounds"))
}

fn pixel_bounds_outward(rect: Rect) -> Result<PixelBounds> {
    let min_x = finite_floor_i32(rect.min().x())?;
    let min_y = finite_floor_i32(rect.min().y())?;
    let max_x = finite_ceil_i32(rect.max().x())?;
    let max_y = finite_ceil_i32(rect.max().y())?;
    PixelBounds::new(min_x, min_y, max_x, max_y)
        .map_err(|error| with_reference_context(error, "round_bounds"))
}

fn finite_floor_i32(value: f64) -> Result<i32> {
    let value = value.floor();
    if value.is_finite() && value >= f64::from(i32::MIN) && value <= f64::from(i32::MAX) {
        Ok(value as i32)
    } else {
        Err(bounds_overflow_error())
    }
}

fn finite_ceil_i32(value: f64) -> Result<i32> {
    let value = value.ceil();
    if value.is_finite() && value >= f64::from(i32::MIN) && value <= f64::from(i32::MAX) {
        Ok(value as i32)
    } else {
        Err(bounds_overflow_error())
    }
}

fn radial_source_bounds(
    output: PixelBounds,
    center: [f64; 2],
    radius: f64,
    k1: f64,
    k2: f64,
) -> Result<PixelBounds> {
    let corners = [
        [f64::from(output.min_x()), f64::from(output.min_y())],
        [f64::from(output.max_x()), f64::from(output.min_y())],
        [f64::from(output.min_x()), f64::from(output.max_y())],
        [f64::from(output.max_x()), f64::from(output.max_y())],
    ];
    let max_radius = corners
        .iter()
        .map(|corner| {
            let x = (corner[0] - center[0]) / radius;
            let y = (corner[1] - center[1]) / radius;
            (x * x + y * y).sqrt()
        })
        .fold(0.0, f64::max);
    validate_radial_monotonic(max_radius, k1, k2)?;
    let squared = max_radius * max_radius;
    let factor = 1.0 + k1 * squared + k2 * squared * squared;
    let mapped_radius = max_radius * factor * radius;
    if !mapped_radius.is_finite() || mapped_radius < 0.0 {
        return Err(radial_mapping_error());
    }
    let rect = Rect::new(
        superi_core::geometry::Point2::new(center[0] - mapped_radius, center[1] - mapped_radius)?,
        superi_core::geometry::Point2::new(center[0] + mapped_radius, center[1] + mapped_radius)?,
    )
    .map_err(|error| with_reference_context(error, "radial_bounds"))?;
    expand_bounds(pixel_bounds_outward(rect)?, 1)
}

fn validate_radial_monotonic(max_radius: f64, k1: f64, k2: f64) -> Result<()> {
    let maximum_s = max_radius * max_radius;
    let derivative = |s: f64| 1.0 + 3.0 * k1 * s + 5.0 * k2 * s * s;
    let mut minimum = derivative(0.0).min(derivative(maximum_s));
    if k2 > 0.0 {
        let vertex = -3.0 * k1 / (10.0 * k2);
        if (0.0..=maximum_s).contains(&vertex) {
            minimum = minimum.min(derivative(vertex));
        }
    }
    if minimum.is_finite() && minimum > 0.0 {
        Ok(())
    } else {
        Err(radial_mapping_error())
    }
}

fn radial_mapping_error() -> Error {
    reference_error(
        ErrorCategory::InvalidInput,
        "radial_distortion",
        "non_monotonic_radial_mapping",
        "radial distortion must remain monotonic and bijective across the requested region",
    )
}

fn bounds_overflow_error() -> Error {
    reference_error(
        ErrorCategory::ResourceExhausted,
        "bounds",
        "bounds_overflow",
        "reference dependency bounds exceed the supported coordinate range",
    )
}

fn allocation_overflow_error() -> Error {
    reference_error(
        ErrorCategory::ResourceExhausted,
        "allocation",
        "allocation_overflow",
        "reference output allocation exceeds the host address space",
    )
}

fn allocation_error() -> Error {
    reference_error(
        ErrorCategory::ResourceExhausted,
        "allocation",
        "allocation_failed",
        "reference output allocation could not be reserved",
    )
}

fn output_range_error() -> Error {
    reference_error(
        ErrorCategory::InvalidInput,
        "output",
        "nonfinite_output",
        "reference operation produced a nonfinite or invalid-alpha output",
    )
}

fn with_reference_context(mut error: Error, operation: &'static str) -> Error {
    error.push_context(ErrorContext::new(COMPONENT, operation));
    error
}

fn reference_error(
    category: ErrorCategory,
    operation: &'static str,
    reason: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, Recoverability::UserCorrectable, message)
        .with_context(ErrorContext::new(COMPONENT, operation).with_field("reason", reason))
}
