//! Editable visual composition state with exact-time remapping and nested resolution.
//!
//! Composition artifacts are workflow-neutral authored state. They retain generic visual payloads,
//! same-composition layer parenting, reusable precomposition references, explicit collapse
//! boundaries, and exact layer-to-source time maps. Resolution emits inspectable structural paths
//! for graph compilation without owning editorial nesting, spatial transform math, or pixels.

use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::marker::PhantomData;

use serde::de::{Error as _, SeqAccess, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::time::{RationalTime, TimeRange, TimeRounding, Timebase};

use crate::authoring::required_text;

const COMPONENT: &str = "superi-effects.composition";

/// Current standalone composition artifact wire revision.
pub const COMPOSITION_ARTIFACT_SCHEMA_REVISION: u32 = 1;
/// Maximum compositions retained by one artifact.
pub const MAX_COMPOSITIONS: usize = 1_024;
/// Maximum bottom-to-top layers retained by one composition.
pub const MAX_LAYERS_PER_COMPOSITION: usize = 65_536;
/// Maximum exact keyframes retained by one layer time map.
pub const MAX_TIME_REMAP_KEYFRAMES: usize = 16_384;

/// Stable identity for one reusable visual composition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CompositionId(u64);

impl CompositionId {
    /// Creates an identity from its persisted integer representation.
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Returns the persisted integer representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for CompositionId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Stable identity for one layer inside one composition.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CompositionLayerId(u64);

impl CompositionLayerId {
    /// Creates an identity from its persisted integer representation.
    #[must_use]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    /// Returns the persisted integer representation.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for CompositionLayerId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Interpolation leaving one exact layer-to-source time keyframe.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TimeRemapInterpolation {
    /// Interpolate source coordinates linearly until the next keyframe.
    Linear,
    /// Hold this source coordinate until the next keyframe.
    Hold,
}

/// One editable exact layer-to-source time relationship.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimeRemapKeyframe {
    layer_time: RationalTime,
    source_time: RationalTime,
    interpolation: TimeRemapInterpolation,
}

impl TimeRemapKeyframe {
    /// Creates one keyframe. Its owning time map validates clocks and ordering.
    #[must_use]
    pub const fn new(
        layer_time: RationalTime,
        source_time: RationalTime,
        interpolation: TimeRemapInterpolation,
    ) -> Self {
        Self {
            layer_time,
            source_time,
            interpolation,
        }
    }

    /// Returns the exact coordinate in the owning layer's composition clock.
    #[must_use]
    pub const fn layer_time(self) -> RationalTime {
        self.layer_time
    }

    /// Returns the exact coordinate in the layer source clock.
    #[must_use]
    pub const fn source_time(self) -> RationalTime {
        self.source_time
    }

    /// Returns interpolation leaving this keyframe.
    #[must_use]
    pub const fn interpolation(self) -> TimeRemapInterpolation {
        self.interpolation
    }
}

/// Exact editable mapping from composition-layer time to source time.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TimeRemap {
    layer_timebase: Timebase,
    source_timebase: Timebase,
    keyframes: Vec<TimeRemapKeyframe>,
}

impl TimeRemap {
    /// Creates a checked map with strictly increasing layer coordinates.
    ///
    /// A single keyframe is a complete freeze map. Coordinates before the first keyframe or after
    /// the last keyframe hold the nearest endpoint.
    pub fn new(keyframes: impl IntoIterator<Item = TimeRemapKeyframe>) -> Result<Self> {
        let keyframes = keyframes.into_iter().collect::<Vec<_>>();
        if keyframes.is_empty() {
            return Err(invalid_error(
                "create_time_remap",
                "empty_time_remap",
                "time remap requires at least one keyframe",
            ));
        }
        if keyframes.len() > MAX_TIME_REMAP_KEYFRAMES {
            return Err(resource_error(
                "create_time_remap",
                "time_remap_keyframe_limit_exceeded",
                "time remap exceeds the supported keyframe bound",
            ));
        }
        let layer_timebase = keyframes[0].layer_time.timebase();
        let source_timebase = keyframes[0].source_time.timebase();
        for (index, keyframe) in keyframes.iter().enumerate() {
            if keyframe.layer_time.timebase() != layer_timebase {
                return Err(indexed_error(
                    "create_time_remap",
                    "mixed_layer_timebases",
                    "time remap layer coordinates must use one timebase",
                    index,
                ));
            }
            if keyframe.source_time.timebase() != source_timebase {
                return Err(indexed_error(
                    "create_time_remap",
                    "mixed_source_timebases",
                    "time remap source coordinates must use one timebase",
                    index,
                ));
            }
            if index > 0 && keyframes[index - 1].layer_time >= keyframe.layer_time {
                return Err(indexed_error(
                    "create_time_remap",
                    "unordered_time_remap_keys",
                    "time remap layer coordinates must be strictly increasing",
                    index,
                ));
            }
        }
        Ok(Self {
            layer_timebase,
            source_timebase,
            keyframes,
        })
    }

    /// Returns the layer-side clock.
    #[must_use]
    pub const fn layer_timebase(&self) -> Timebase {
        self.layer_timebase
    }

    /// Returns the source-side clock.
    #[must_use]
    pub const fn source_timebase(&self) -> Timebase {
        self.source_timebase
    }

    /// Returns keyframes in authored layer-time order.
    #[must_use]
    pub fn keyframes(&self) -> &[TimeRemapKeyframe] {
        &self.keyframes
    }

    /// Maps one layer coordinate to exact source time using the requested rounding policy.
    pub fn sample(
        &self,
        layer_time: RationalTime,
        rounding: TimeRounding,
    ) -> Result<TimeRemapSample> {
        if layer_time.timebase() != self.layer_timebase {
            return Err(invalid_error(
                "sample_time_remap",
                "layer_timebase_mismatch",
                "sample time must use the time remap layer clock",
            ));
        }
        if layer_time <= self.keyframes[0].layer_time {
            return Ok(TimeRemapSample {
                source_time: self.keyframes[0].source_time,
                segment_index: 0,
                interpolation: TimeRemapInterpolation::Hold,
            });
        }
        let last_index = self.keyframes.len() - 1;
        if layer_time >= self.keyframes[last_index].layer_time {
            return Ok(TimeRemapSample {
                source_time: self.keyframes[last_index].source_time,
                segment_index: last_index,
                interpolation: TimeRemapInterpolation::Hold,
            });
        }
        let next_index = self
            .keyframes
            .partition_point(|keyframe| keyframe.layer_time <= layer_time);
        let segment_index = next_index - 1;
        let start = self.keyframes[segment_index];
        let end = self.keyframes[next_index];
        let source_time = match start.interpolation {
            TimeRemapInterpolation::Hold => start.source_time,
            TimeRemapInterpolation::Linear => {
                let layer_offset =
                    i128::from(layer_time.value()) - i128::from(start.layer_time.value());
                let layer_span =
                    i128::from(end.layer_time.value()) - i128::from(start.layer_time.value());
                let source_span =
                    i128::from(end.source_time.value()) - i128::from(start.source_time.value());
                let scaled = rounded_divide(
                    layer_offset * source_span,
                    layer_span,
                    rounding,
                    "sample_time_remap",
                )?;
                let value = i128::from(start.source_time.value())
                    .checked_add(scaled)
                    .and_then(|value| i64::try_from(value).ok())
                    .ok_or_else(|| {
                        resource_error(
                            "sample_time_remap",
                            "time_remap_overflow",
                            "mapped source time exceeds the supported coordinate range",
                        )
                    })?;
                RationalTime::new(value, self.source_timebase)
            }
        };
        Ok(TimeRemapSample {
            source_time,
            segment_index,
            interpolation: start.interpolation,
        })
    }
}

/// Inspectable result of sampling one time-remap segment.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TimeRemapSample {
    source_time: RationalTime,
    segment_index: usize,
    interpolation: TimeRemapInterpolation,
}

impl TimeRemapSample {
    /// Returns the mapped exact source coordinate.
    #[must_use]
    pub const fn source_time(self) -> RationalTime {
        self.source_time
    }

    /// Returns the keyframe index that supplied the sampled segment or endpoint hold.
    #[must_use]
    pub const fn segment_index(self) -> usize {
        self.segment_index
    }

    /// Returns the interpolation applied to this sample.
    #[must_use]
    pub const fn interpolation(self) -> TimeRemapInterpolation {
        self.interpolation
    }
}

/// Whether a precomposition remains one intermediate boundary or exposes its child layers.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PrecompositionCollapse {
    /// Retain the nested composition as one reusable intermediate surface.
    PreserveBoundary,
    /// Expose nested layers when no layer operation requires an intermediate surface.
    Collapse,
}

/// Whether layer-level visual operations require an intermediate surface.
#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum LayerIsolation {
    /// Layer state may participate in collapsed nested resolution.
    PassThrough,
    /// Masks, effects, or another layer operation require one intermediate surface.
    RequiresIntermediateSurface,
}

/// Authored source and complete generic visual payload for one layer.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum LayerSource<T> {
    /// A leaf visual source with complete editable layer state.
    Visual(T),
    /// One reusable nested composition instance with complete editable instance state.
    Precomposition {
        /// Referenced composition identity.
        composition_id: CompositionId,
        /// Complete editable state attached to this precomposition layer.
        payload: T,
    },
}

impl<T> LayerSource<T> {
    /// Returns the complete generic visual state attached to this layer.
    #[must_use]
    pub const fn payload(&self) -> &T {
        match self {
            Self::Visual(payload) | Self::Precomposition { payload, .. } => payload,
        }
    }

    /// Returns the referenced composition for a precomposition source.
    #[must_use]
    pub const fn precomposition_id(&self) -> Option<CompositionId> {
        match self {
            Self::Visual(_) => None,
            Self::Precomposition { composition_id, .. } => Some(*composition_id),
        }
    }
}

/// One editable layer in bottom-to-top composition order.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompositionLayer<T> {
    id: CompositionLayerId,
    name: String,
    active_range: TimeRange,
    parent_id: Option<CompositionLayerId>,
    source: LayerSource<T>,
    time_remap: TimeRemap,
    collapse: PrecompositionCollapse,
    isolation: LayerIsolation,
}

impl<T> CompositionLayer<T> {
    /// Creates one leaf visual layer.
    pub fn visual(
        id: CompositionLayerId,
        name: impl Into<String>,
        active_range: TimeRange,
        payload: T,
        time_remap: TimeRemap,
    ) -> Result<Self> {
        Self::from_parts(
            id,
            name.into(),
            active_range,
            None,
            LayerSource::Visual(payload),
            time_remap,
            PrecompositionCollapse::PreserveBoundary,
            LayerIsolation::PassThrough,
        )
    }

    /// Creates one reusable precomposition layer with explicit collapse and isolation controls.
    #[allow(clippy::too_many_arguments)]
    pub fn precomposition(
        id: CompositionLayerId,
        name: impl Into<String>,
        active_range: TimeRange,
        composition_id: CompositionId,
        payload: T,
        time_remap: TimeRemap,
        collapse: PrecompositionCollapse,
        isolation: LayerIsolation,
    ) -> Result<Self> {
        Self::from_parts(
            id,
            name.into(),
            active_range,
            None,
            LayerSource::Precomposition {
                composition_id,
                payload,
            },
            time_remap,
            collapse,
            isolation,
        )
    }

    #[allow(clippy::too_many_arguments)]
    fn from_parts(
        id: CompositionLayerId,
        name: String,
        active_range: TimeRange,
        parent_id: Option<CompositionLayerId>,
        source: LayerSource<T>,
        time_remap: TimeRemap,
        collapse: PrecompositionCollapse,
        isolation: LayerIsolation,
    ) -> Result<Self> {
        let name = required_text("create_composition_layer", "name", name)?;
        if active_range.is_empty() {
            return Err(layer_error(
                "create_composition_layer",
                id,
                "empty_layer_range",
                "composition layer active range must not be empty",
            ));
        }
        if active_range.timebase() != time_remap.layer_timebase {
            return Err(layer_error(
                "create_composition_layer",
                id,
                "layer_timebase_mismatch",
                "layer active range and time remap must use the same layer clock",
            ));
        }
        if matches!(source, LayerSource::Visual(_))
            && (collapse != PrecompositionCollapse::PreserveBoundary
                || isolation != LayerIsolation::PassThrough)
        {
            return Err(layer_error(
                "create_composition_layer",
                id,
                "precomposition_control_on_visual",
                "collapse and isolation controls apply only to precomposition layers",
            ));
        }
        Ok(Self {
            id,
            name,
            active_range,
            parent_id,
            source,
            time_remap,
            collapse,
            isolation,
        })
    }

    /// Returns this layer's composition-local identity.
    #[must_use]
    pub const fn id(&self) -> CompositionLayerId {
        self.id
    }

    /// Returns the user-facing editable layer name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the half-open active range in the owning composition clock.
    #[must_use]
    pub const fn active_range(&self) -> TimeRange {
        self.active_range
    }

    /// Returns the optional same-composition parent layer identity.
    #[must_use]
    pub const fn parent_id(&self) -> Option<CompositionLayerId> {
        self.parent_id
    }

    /// Returns the complete editable visual source state.
    #[must_use]
    pub const fn source(&self) -> &LayerSource<T> {
        &self.source
    }

    /// Returns the complete editable visual payload.
    #[must_use]
    pub const fn payload(&self) -> &T {
        self.source.payload()
    }

    /// Returns the exact editable layer-to-source time mapping.
    #[must_use]
    pub const fn time_remap(&self) -> &TimeRemap {
        &self.time_remap
    }

    /// Returns the authored precomposition collapse intent.
    #[must_use]
    pub const fn collapse(&self) -> PrecompositionCollapse {
        self.collapse
    }

    /// Returns the authored layer isolation requirement.
    #[must_use]
    pub const fn isolation(&self) -> LayerIsolation {
        self.isolation
    }

    /// Returns a copy with a new same-composition parent relationship.
    #[must_use]
    pub fn with_parent(mut self, parent_id: Option<CompositionLayerId>) -> Self {
        self.parent_id = parent_id;
        self
    }

    /// Returns a copy with a new complete visual payload.
    #[must_use]
    pub fn with_payload(mut self, payload: T) -> Self {
        match &mut self.source {
            LayerSource::Visual(current)
            | LayerSource::Precomposition {
                payload: current, ..
            } => {
                *current = payload;
            }
        }
        self
    }

    /// Returns a copy with a new editable name.
    pub fn with_name(mut self, name: impl Into<String>) -> Result<Self> {
        self.name = required_text("rename_composition_layer", "name", name.into())?;
        Ok(self)
    }

    /// Returns a copy with a new active range using the existing time-map layer clock.
    pub fn with_active_range(mut self, active_range: TimeRange) -> Result<Self> {
        if active_range.is_empty() {
            return Err(layer_error(
                "edit_layer_range",
                self.id,
                "empty_layer_range",
                "composition layer active range must not be empty",
            ));
        }
        if active_range.timebase() != self.time_remap.layer_timebase {
            return Err(layer_error(
                "edit_layer_range",
                self.id,
                "layer_timebase_mismatch",
                "layer active range and time remap must use the same layer clock",
            ));
        }
        self.active_range = active_range;
        Ok(self)
    }

    /// Returns a copy with a new exact layer-to-source mapping.
    pub fn with_time_remap(mut self, time_remap: TimeRemap) -> Result<Self> {
        if self.active_range.timebase() != time_remap.layer_timebase {
            return Err(layer_error(
                "edit_layer_time_remap",
                self.id,
                "layer_timebase_mismatch",
                "layer active range and time remap must use the same layer clock",
            ));
        }
        self.time_remap = time_remap;
        Ok(self)
    }

    /// Returns a copy converted to a leaf visual source.
    #[must_use]
    pub fn with_visual_source(mut self, payload: T) -> Self {
        self.source = LayerSource::Visual(payload);
        self.collapse = PrecompositionCollapse::PreserveBoundary;
        self.isolation = LayerIsolation::PassThrough;
        self
    }

    /// Returns a copy converted to a reusable precomposition source.
    #[must_use]
    pub fn with_precomposition_source(
        mut self,
        composition_id: CompositionId,
        payload: T,
        collapse: PrecompositionCollapse,
        isolation: LayerIsolation,
    ) -> Self {
        self.source = LayerSource::Precomposition {
            composition_id,
            payload,
        };
        self.collapse = collapse;
        self.isolation = isolation;
        self
    }

    /// Returns a copy with new precomposition collapse intent.
    pub fn with_collapse(mut self, collapse: PrecompositionCollapse) -> Result<Self> {
        if matches!(self.source, LayerSource::Visual(_)) {
            return Err(layer_error(
                "edit_layer_collapse",
                self.id,
                "collapse_requires_precomposition",
                "collapse controls apply only to a precomposition layer",
            ));
        }
        self.collapse = collapse;
        Ok(self)
    }

    /// Returns a copy with a new precomposition isolation requirement.
    pub fn with_isolation(mut self, isolation: LayerIsolation) -> Result<Self> {
        if matches!(self.source, LayerSource::Visual(_)) {
            return Err(layer_error(
                "edit_layer_isolation",
                self.id,
                "isolation_requires_precomposition",
                "isolation controls apply only to a precomposition layer",
            ));
        }
        self.isolation = isolation;
        Ok(self)
    }
}

/// One reusable visual composition with bottom-to-top layers.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Composition<T> {
    id: CompositionId,
    name: String,
    range: TimeRange,
    layers: Vec<CompositionLayer<T>>,
}

impl<T> Composition<T> {
    /// Creates one composition and validates its local layer-parent DAG.
    pub fn new(
        id: CompositionId,
        name: impl Into<String>,
        range: TimeRange,
        layers: impl IntoIterator<Item = CompositionLayer<T>>,
    ) -> Result<Self> {
        Self::from_parts(id, name.into(), range, layers.into_iter().collect())
    }

    fn from_parts(
        id: CompositionId,
        name: String,
        range: TimeRange,
        layers: Vec<CompositionLayer<T>>,
    ) -> Result<Self> {
        let name = required_text("create_composition", "name", name)?;
        if range.is_empty() {
            return Err(composition_id_error(
                "create_composition",
                id,
                "empty_composition_range",
                "composition range must not be empty",
            ));
        }
        if layers.len() > MAX_LAYERS_PER_COMPOSITION {
            return Err(resource_error(
                "create_composition",
                "composition_layer_limit_exceeded",
                "composition exceeds the supported layer bound",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "create_composition")
                    .with_field("composition_id", id.to_string()),
            ));
        }
        let composition = Self {
            id,
            name,
            range,
            layers,
        };
        composition.validate_layers()?;
        Ok(composition)
    }

    /// Returns the reusable composition identity.
    #[must_use]
    pub const fn id(&self) -> CompositionId {
        self.id
    }

    /// Returns the user-facing editable name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the half-open composition range and clock.
    #[must_use]
    pub const fn range(&self) -> TimeRange {
        self.range
    }

    /// Returns the composition clock.
    #[must_use]
    pub const fn timebase(&self) -> Timebase {
        self.range.timebase()
    }

    /// Returns layers in bottom-to-top authored order.
    #[must_use]
    pub fn layers(&self) -> &[CompositionLayer<T>] {
        &self.layers
    }

    /// Looks up one composition-local layer.
    #[must_use]
    pub fn layer(&self, id: CompositionLayerId) -> Option<&CompositionLayer<T>> {
        self.layers.iter().find(|layer| layer.id == id)
    }

    /// Returns a layer's parent chain in root-to-direct order.
    pub fn parent_chain(&self, id: CompositionLayerId) -> Result<Vec<CompositionLayerId>> {
        let mut current = self.layer(id).ok_or_else(|| {
            layer_error(
                "inspect_parent_chain",
                id,
                "layer_not_found",
                "composition layer does not exist",
            )
        })?;
        let mut chain = Vec::new();
        while let Some(parent_id) = current.parent_id {
            chain.push(parent_id);
            current = self
                .layer(parent_id)
                .expect("composition validation retains every parent layer");
        }
        chain.reverse();
        Ok(chain)
    }

    fn validate_layers(&self) -> Result<()> {
        let mut identities = BTreeSet::new();
        for layer in &self.layers {
            if layer.active_range.timebase() != self.timebase() {
                return Err(layer_error(
                    "create_composition",
                    layer.id,
                    "composition_timebase_mismatch",
                    "every layer active range must use the composition clock",
                ));
            }
            if !identities.insert(layer.id) {
                return Err(layer_error(
                    "create_composition",
                    layer.id,
                    "duplicate_layer_id",
                    "composition layer identity appears more than once",
                ));
            }
        }
        for layer in &self.layers {
            if let Some(parent_id) = layer.parent_id {
                if parent_id == layer.id {
                    return Err(layer_error(
                        "create_composition",
                        layer.id,
                        "self_parent",
                        "composition layer cannot parent itself",
                    ));
                }
                if !identities.contains(&parent_id) {
                    return Err(layer_error(
                        "create_composition",
                        layer.id,
                        "missing_parent_layer",
                        "composition layer parent does not exist in the same composition",
                    )
                    .with_context(
                        ErrorContext::new(COMPONENT, "create_composition")
                            .with_field("parent_id", parent_id.to_string()),
                    ));
                }
            }
        }
        let mut complete = BTreeSet::new();
        for layer in &self.layers {
            let mut path = BTreeSet::new();
            let mut current = layer;
            while let Some(parent_id) = current.parent_id {
                if complete.contains(&parent_id) {
                    break;
                }
                if !path.insert(current.id) {
                    return Err(layer_error(
                        "create_composition",
                        layer.id,
                        "layer_parent_cycle",
                        "composition layer parenting must remain acyclic",
                    ));
                }
                current = self
                    .layer(parent_id)
                    .expect("parent existence was validated before cycle traversal");
            }
            complete.extend(path);
            complete.insert(current.id);
        }
        Ok(())
    }
}

impl<T: Clone> Composition<T> {
    /// Returns a copy with a new editable composition name.
    pub fn with_name(&self, name: impl Into<String>) -> Result<Self> {
        Self::from_parts(self.id, name.into(), self.range, self.layers.clone())
    }

    /// Returns a copy with a new range using the same clock as every layer.
    pub fn with_range(&self, range: TimeRange) -> Result<Self> {
        Self::from_parts(self.id, self.name.clone(), range, self.layers.clone())
    }

    /// Inserts one layer at a bottom-to-top position.
    pub fn with_layer(&self, layer: CompositionLayer<T>, position: usize) -> Result<Self> {
        if position > self.layers.len() {
            return Err(invalid_error(
                "insert_composition_layer",
                "layer_position_out_of_bounds",
                "layer insertion position exceeds composition length",
            ));
        }
        let mut layers = self.layers.clone();
        layers.insert(position, layer);
        Self::from_parts(self.id, self.name.clone(), self.range, layers)
    }

    /// Replaces one layer while retaining its current bottom-to-top position.
    pub fn with_replaced_layer(&self, replacement: CompositionLayer<T>) -> Result<Self> {
        let position = self
            .layers
            .iter()
            .position(|layer| layer.id == replacement.id)
            .ok_or_else(|| {
                layer_error(
                    "replace_composition_layer",
                    replacement.id,
                    "layer_not_found",
                    "composition layer does not exist",
                )
            })?;
        let mut layers = self.layers.clone();
        layers[position] = replacement;
        Self::from_parts(self.id, self.name.clone(), self.range, layers)
    }

    /// Removes one unreferenced layer.
    pub fn without_layer(&self, id: CompositionLayerId) -> Result<Self> {
        if self.layers.iter().any(|layer| layer.parent_id == Some(id)) {
            return Err(layer_error(
                "remove_composition_layer",
                id,
                "layer_has_children",
                "composition layer cannot be removed while another layer names it as parent",
            ));
        }
        let position = self
            .layers
            .iter()
            .position(|layer| layer.id == id)
            .ok_or_else(|| {
                layer_error(
                    "remove_composition_layer",
                    id,
                    "layer_not_found",
                    "composition layer does not exist",
                )
            })?;
        let mut layers = self.layers.clone();
        layers.remove(position);
        Self::from_parts(self.id, self.name.clone(), self.range, layers)
    }

    /// Moves one layer to a new bottom-to-top position.
    pub fn with_reordered_layer(&self, id: CompositionLayerId, position: usize) -> Result<Self> {
        if position >= self.layers.len() {
            return Err(invalid_error(
                "reorder_composition_layer",
                "layer_position_out_of_bounds",
                "layer destination position exceeds composition length",
            ));
        }
        let current = self
            .layers
            .iter()
            .position(|layer| layer.id == id)
            .ok_or_else(|| {
                layer_error(
                    "reorder_composition_layer",
                    id,
                    "layer_not_found",
                    "composition layer does not exist",
                )
            })?;
        let mut layers = self.layers.clone();
        let layer = layers.remove(current);
        layers.insert(position, layer);
        Self::from_parts(self.id, self.name.clone(), self.range, layers)
    }
}

/// Why one precomposition remains an inspectable intermediate boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PrecompositionBoundaryReason {
    /// The authored collapse control preserves the boundary.
    CollapseDisabled,
    /// Layer-level masks, effects, or other operations require an intermediate surface.
    IntermediateSurfaceRequired,
}

/// Final structural kind produced for one resolved visual output.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ResolvedLayerKind {
    /// One leaf visual source.
    Visual,
    /// One retained reusable precomposition boundary.
    PrecompositionBoundary {
        /// Referenced composition identity.
        composition_id: CompositionId,
        /// Rule that retained the intermediate boundary.
        reason: PrecompositionBoundaryReason,
    },
}

/// One inspectable layer step retained through collapsed nested resolution.
#[derive(Debug, Eq, PartialEq)]
pub struct ResolvedLayerStep<'a, T> {
    composition_id: CompositionId,
    layer: &'a CompositionLayer<T>,
    source_time: RationalTime,
    parent_chain: Vec<CompositionLayerId>,
}

impl<T> Clone for ResolvedLayerStep<'_, T> {
    fn clone(&self) -> Self {
        Self {
            composition_id: self.composition_id,
            layer: self.layer,
            source_time: self.source_time,
            parent_chain: self.parent_chain.clone(),
        }
    }
}

impl<'a, T> ResolvedLayerStep<'a, T> {
    /// Returns the composition containing this step.
    #[must_use]
    pub const fn composition_id(&self) -> CompositionId {
        self.composition_id
    }

    /// Returns the complete authored layer.
    #[must_use]
    pub const fn layer(&self) -> &'a CompositionLayer<T> {
        self.layer
    }

    /// Returns the exact source coordinate mapped at this nesting step.
    #[must_use]
    pub const fn source_time(&self) -> RationalTime {
        self.source_time
    }

    /// Returns same-composition parents in root-to-direct order.
    #[must_use]
    pub fn parent_chain(&self) -> &[CompositionLayerId] {
        &self.parent_chain
    }
}

/// One bottom-to-top visual output with its complete nested authored path.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedLayer<'a, T> {
    kind: ResolvedLayerKind,
    path: Vec<ResolvedLayerStep<'a, T>>,
}

impl<'a, T> ResolvedLayer<'a, T> {
    /// Returns the structural output kind.
    #[must_use]
    pub const fn kind(&self) -> ResolvedLayerKind {
        self.kind
    }

    /// Returns every collapsed ancestor and the final output layer in root-to-leaf order.
    #[must_use]
    pub fn path(&self) -> &[ResolvedLayerStep<'a, T>] {
        &self.path
    }

    /// Returns the final layer's complete editable visual payload.
    #[must_use]
    pub fn payload(&self) -> &T {
        self.path
            .last()
            .expect("resolved layer paths are never empty")
            .layer
            .payload()
    }

    /// Returns the final layer's mapped exact source coordinate.
    #[must_use]
    pub fn source_time(&self) -> RationalTime {
        self.path
            .last()
            .expect("resolved layer paths are never empty")
            .source_time
    }
}

/// Structural resolution of one exact root composition frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedCompositionFrame<'a, T> {
    composition_id: CompositionId,
    time: RationalTime,
    layers: Vec<ResolvedLayer<'a, T>>,
}

impl<'a, T> ResolvedCompositionFrame<'a, T> {
    /// Returns the resolved root composition identity.
    #[must_use]
    pub const fn composition_id(&self) -> CompositionId {
        self.composition_id
    }

    /// Returns the exact root composition coordinate.
    #[must_use]
    pub const fn time(&self) -> RationalTime {
        self.time
    }

    /// Returns visual outputs in deterministic bottom-to-top order.
    #[must_use]
    pub fn layers(&self) -> &[ResolvedLayer<'a, T>] {
        &self.layers
    }
}

/// Complete reusable visual composition artifact.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CompositionArtifact<T> {
    root_id: CompositionId,
    content_revision: u64,
    compositions: Vec<Composition<T>>,
}

impl<T> CompositionArtifact<T> {
    /// Creates a revision-zero artifact and validates every local and nested relationship.
    pub fn new(
        root_id: CompositionId,
        compositions: impl IntoIterator<Item = Composition<T>>,
    ) -> Result<Self> {
        Self::from_parts(root_id, 0, compositions.into_iter().collect())
    }

    fn from_parts(
        root_id: CompositionId,
        content_revision: u64,
        compositions: Vec<Composition<T>>,
    ) -> Result<Self> {
        if compositions.is_empty() {
            return Err(invalid_error(
                "create_composition_artifact",
                "empty_composition_artifact",
                "composition artifact requires at least one composition",
            ));
        }
        if compositions.len() > MAX_COMPOSITIONS {
            return Err(resource_error(
                "create_composition_artifact",
                "composition_limit_exceeded",
                "composition artifact exceeds the supported composition bound",
            ));
        }
        let mut by_id = BTreeMap::new();
        for composition in compositions {
            let id = composition.id;
            if by_id.insert(id, composition).is_some() {
                return Err(composition_id_error(
                    "create_composition_artifact",
                    id,
                    "duplicate_composition_id",
                    "composition identity appears more than once",
                ));
            }
        }
        if !by_id.contains_key(&root_id) {
            return Err(composition_id_error(
                "create_composition_artifact",
                root_id,
                "missing_root_composition",
                "root composition does not exist in the artifact",
            ));
        }
        let artifact = Self {
            root_id,
            content_revision,
            compositions: by_id.into_values().collect(),
        };
        artifact.validate_relationships()?;
        Ok(artifact)
    }

    /// Returns the root composition identity.
    #[must_use]
    pub const fn root_id(&self) -> CompositionId {
        self.root_id
    }

    /// Returns the immutable authored-content revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.content_revision
    }

    /// Returns reusable compositions in canonical identity order.
    #[must_use]
    pub fn compositions(&self) -> &[Composition<T>] {
        &self.compositions
    }

    /// Looks up one reusable composition.
    #[must_use]
    pub fn composition(&self, id: CompositionId) -> Option<&Composition<T>> {
        self.compositions
            .binary_search_by_key(&id, |composition| composition.id)
            .ok()
            .map(|index| &self.compositions[index])
    }

    /// Resolves one exact root frame into inspectable bottom-to-top structural outputs.
    pub fn resolve_frame(
        &self,
        time: RationalTime,
        rounding: TimeRounding,
    ) -> Result<ResolvedCompositionFrame<'_, T>> {
        let root = self
            .composition(self.root_id)
            .expect("artifact validation retains its root composition");
        let time = time.checked_rescale(root.timebase(), rounding)?;
        if !root.range.contains(time)? {
            return Err(composition_id_error(
                "resolve_composition_frame",
                root.id,
                "time_outside_composition",
                "requested time lies outside the root composition range",
            ));
        }
        let mut layers = Vec::new();
        let mut stack = BTreeSet::new();
        self.resolve_composition(root, time, rounding, &mut stack, Vec::new(), &mut layers)?;
        Ok(ResolvedCompositionFrame {
            composition_id: root.id,
            time,
            layers,
        })
    }

    fn resolve_composition<'a>(
        &'a self,
        composition: &'a Composition<T>,
        time: RationalTime,
        rounding: TimeRounding,
        stack: &mut BTreeSet<CompositionId>,
        prefix: Vec<ResolvedLayerStep<'a, T>>,
        output: &mut Vec<ResolvedLayer<'a, T>>,
    ) -> Result<()> {
        if !stack.insert(composition.id) {
            return Err(composition_id_error(
                "resolve_composition_frame",
                composition.id,
                "composition_cycle",
                "recursive precomposition resolution is not allowed",
            ));
        }
        for layer in &composition.layers {
            if !layer.active_range.contains(time)? {
                continue;
            }
            let sample = layer.time_remap.sample(time, rounding)?;
            let mut path = prefix.clone();
            path.push(ResolvedLayerStep {
                composition_id: composition.id,
                layer,
                source_time: sample.source_time,
                parent_chain: composition.parent_chain(layer.id)?,
            });
            match &layer.source {
                LayerSource::Visual(_) => output.push(ResolvedLayer {
                    kind: ResolvedLayerKind::Visual,
                    path,
                }),
                LayerSource::Precomposition { composition_id, .. } => {
                    let child = self
                        .composition(*composition_id)
                        .expect("artifact validation retains referenced compositions");
                    if !child.range.contains(sample.source_time)? {
                        return Err(layer_error(
                            "resolve_composition_frame",
                            layer.id,
                            "mapped_time_outside_precomposition",
                            "mapped source time lies outside the referenced composition range",
                        )
                        .with_context(
                            ErrorContext::new(COMPONENT, "resolve_composition_frame")
                                .with_field("composition_id", composition_id.to_string()),
                        ));
                    }
                    let boundary_reason = match (layer.collapse, layer.isolation) {
                        (PrecompositionCollapse::PreserveBoundary, _) => {
                            Some(PrecompositionBoundaryReason::CollapseDisabled)
                        }
                        (
                            PrecompositionCollapse::Collapse,
                            LayerIsolation::RequiresIntermediateSurface,
                        ) => Some(PrecompositionBoundaryReason::IntermediateSurfaceRequired),
                        (PrecompositionCollapse::Collapse, LayerIsolation::PassThrough) => None,
                    };
                    if let Some(reason) = boundary_reason {
                        output.push(ResolvedLayer {
                            kind: ResolvedLayerKind::PrecompositionBoundary {
                                composition_id: *composition_id,
                                reason,
                            },
                            path,
                        });
                    } else {
                        self.resolve_composition(
                            child,
                            sample.source_time,
                            rounding,
                            stack,
                            path,
                            output,
                        )?;
                    }
                }
            }
        }
        stack.remove(&composition.id);
        Ok(())
    }

    fn validate_relationships(&self) -> Result<()> {
        for composition in &self.compositions {
            for layer in &composition.layers {
                let Some(target_id) = layer.source.precomposition_id() else {
                    continue;
                };
                let target = self.composition(target_id).ok_or_else(|| {
                    composition_id_error(
                        "create_composition_artifact",
                        target_id,
                        "missing_precomposition",
                        "precomposition layer references a composition absent from the artifact",
                    )
                    .with_context(
                        ErrorContext::new(COMPONENT, "create_composition_artifact")
                            .with_field("owner_composition_id", composition.id.to_string())
                            .with_field("layer_id", layer.id.to_string()),
                    )
                })?;
                if layer.time_remap.source_timebase != target.timebase() {
                    return Err(layer_error(
                        "create_composition_artifact",
                        layer.id,
                        "precomposition_timebase_mismatch",
                        "precomposition time-remap source clock must match its target composition",
                    ));
                }
                let target_end = target.range.end_exclusive()?;
                let layer_end = layer.active_range.end_exclusive()?;
                for (index, keyframe) in layer.time_remap.keyframes.iter().enumerate() {
                    let inside = target.range.contains(keyframe.source_time)?;
                    let inactive_end = index + 1 == layer.time_remap.keyframes.len()
                        && keyframe.layer_time >= layer_end
                        && keyframe.source_time == target_end;
                    if !inside && !inactive_end {
                        return Err(indexed_layer_error(
                            "create_composition_artifact",
                            layer.id,
                            "precomposition_time_out_of_range",
                            "precomposition time-remap key lies outside its target composition",
                            index,
                        ));
                    }
                }
            }
        }
        let mut complete = BTreeSet::new();
        for composition in &self.compositions {
            self.validate_composition_dag(composition.id, &mut BTreeSet::new(), &mut complete)?;
        }
        Ok(())
    }

    fn validate_composition_dag(
        &self,
        id: CompositionId,
        visiting: &mut BTreeSet<CompositionId>,
        complete: &mut BTreeSet<CompositionId>,
    ) -> Result<()> {
        if complete.contains(&id) {
            return Ok(());
        }
        if !visiting.insert(id) {
            return Err(composition_id_error(
                "create_composition_artifact",
                id,
                "composition_cycle",
                "precomposition relationships must remain acyclic",
            ));
        }
        let composition = self
            .composition(id)
            .expect("relationship validation starts from known compositions");
        for target in composition
            .layers
            .iter()
            .filter_map(|layer| layer.source.precomposition_id())
        {
            self.validate_composition_dag(target, visiting, complete)?;
        }
        visiting.remove(&id);
        complete.insert(id);
        Ok(())
    }
}

impl<T: Clone> CompositionArtifact<T> {
    /// Adds one reusable composition and advances the content revision.
    pub fn with_composition(&self, composition: Composition<T>) -> Result<Self> {
        if self.composition(composition.id).is_some() {
            return Err(composition_id_error(
                "add_composition",
                composition.id,
                "duplicate_composition_id",
                "composition identity already exists",
            ));
        }
        let mut compositions = self.compositions.clone();
        compositions.push(composition);
        self.rebuild(self.root_id, compositions)
    }

    /// Replaces one reusable composition and advances the content revision.
    pub fn with_replaced_composition(&self, replacement: Composition<T>) -> Result<Self> {
        let position = self
            .compositions
            .iter()
            .position(|composition| composition.id == replacement.id)
            .ok_or_else(|| {
                composition_id_error(
                    "replace_composition",
                    replacement.id,
                    "composition_not_found",
                    "composition does not exist in the artifact",
                )
            })?;
        let mut compositions = self.compositions.clone();
        compositions[position] = replacement;
        self.rebuild(self.root_id, compositions)
    }

    /// Removes one unreferenced nonroot composition and advances the content revision.
    pub fn without_composition(&self, id: CompositionId) -> Result<Self> {
        if id == self.root_id {
            return Err(composition_id_error(
                "remove_composition",
                id,
                "cannot_remove_root_composition",
                "root composition cannot be removed",
            ));
        }
        let position = self
            .compositions
            .iter()
            .position(|composition| composition.id == id)
            .ok_or_else(|| {
                composition_id_error(
                    "remove_composition",
                    id,
                    "composition_not_found",
                    "composition does not exist in the artifact",
                )
            })?;
        let mut compositions = self.compositions.clone();
        compositions.remove(position);
        self.rebuild(self.root_id, compositions)
    }

    /// Selects another existing composition as root and advances the content revision.
    pub fn with_root(&self, root_id: CompositionId) -> Result<Self> {
        self.rebuild(root_id, self.compositions.clone())
    }

    fn rebuild(&self, root_id: CompositionId, compositions: Vec<Composition<T>>) -> Result<Self> {
        let content_revision = self.content_revision.checked_add(1).ok_or_else(|| {
            resource_error(
                "edit_composition_artifact",
                "revision_overflow",
                "composition artifact revision cannot advance beyond its integer bound",
            )
        })?;
        Self::from_parts(root_id, content_revision, compositions)
    }
}

impl<T: Serialize> Serialize for CompositionArtifact<T> {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        CompositionArtifactWireRef::from(self).serialize(serializer)
    }
}

impl<'de, T> Deserialize<'de> for CompositionArtifact<T>
where
    T: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        CompositionArtifactWire::deserialize(deserializer)?
            .into_artifact()
            .map_err(D::Error::custom)
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct CompositionArtifactWireRef<'a, T> {
    schema_revision: u32,
    root_id: u64,
    content_revision: u64,
    compositions: Vec<CompositionWireRef<'a, T>>,
}

impl<'a, T> From<&'a CompositionArtifact<T>> for CompositionArtifactWireRef<'a, T> {
    fn from(artifact: &'a CompositionArtifact<T>) -> Self {
        Self {
            schema_revision: COMPOSITION_ARTIFACT_SCHEMA_REVISION,
            root_id: artifact.root_id.get(),
            content_revision: artifact.content_revision,
            compositions: artifact
                .compositions
                .iter()
                .map(CompositionWireRef::from)
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct CompositionWireRef<'a, T> {
    id: u64,
    name: &'a str,
    range: TimeRange,
    layers: Vec<CompositionLayerWireRef<'a, T>>,
}

impl<'a, T> From<&'a Composition<T>> for CompositionWireRef<'a, T> {
    fn from(composition: &'a Composition<T>) -> Self {
        Self {
            id: composition.id.get(),
            name: &composition.name,
            range: composition.range,
            layers: composition
                .layers
                .iter()
                .map(CompositionLayerWireRef::from)
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct CompositionLayerWireRef<'a, T> {
    id: u64,
    name: &'a str,
    active_range: TimeRange,
    parent_id: Option<u64>,
    source: LayerSourceWireRef<'a, T>,
    time_remap: TimeRemapWireRef<'a>,
    collapse: PrecompositionCollapse,
    isolation: LayerIsolation,
}

impl<'a, T> From<&'a CompositionLayer<T>> for CompositionLayerWireRef<'a, T> {
    fn from(layer: &'a CompositionLayer<T>) -> Self {
        Self {
            id: layer.id.get(),
            name: &layer.name,
            active_range: layer.active_range,
            parent_id: layer.parent_id.map(CompositionLayerId::get),
            source: LayerSourceWireRef::from(&layer.source),
            time_remap: TimeRemapWireRef::from(&layer.time_remap),
            collapse: layer.collapse,
            isolation: layer.isolation,
        }
    }
}

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum LayerSourceWireRef<'a, T> {
    Visual { payload: &'a T },
    Precomposition { composition_id: u64, payload: &'a T },
}

impl<'a, T> From<&'a LayerSource<T>> for LayerSourceWireRef<'a, T> {
    fn from(source: &'a LayerSource<T>) -> Self {
        match source {
            LayerSource::Visual(payload) => Self::Visual { payload },
            LayerSource::Precomposition {
                composition_id,
                payload,
            } => Self::Precomposition {
                composition_id: composition_id.get(),
                payload,
            },
        }
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct TimeRemapWireRef<'a> {
    keyframes: Vec<TimeRemapKeyframeWireRef<'a>>,
}

impl<'a> From<&'a TimeRemap> for TimeRemapWireRef<'a> {
    fn from(time_remap: &'a TimeRemap) -> Self {
        Self {
            keyframes: time_remap
                .keyframes
                .iter()
                .map(TimeRemapKeyframeWireRef::from)
                .collect(),
        }
    }
}

#[derive(Serialize)]
#[serde(deny_unknown_fields)]
struct TimeRemapKeyframeWireRef<'a> {
    layer_time: &'a RationalTime,
    source_time: &'a RationalTime,
    interpolation: TimeRemapInterpolation,
}

impl<'a> From<&'a TimeRemapKeyframe> for TimeRemapKeyframeWireRef<'a> {
    fn from(keyframe: &'a TimeRemapKeyframe) -> Self {
        Self {
            layer_time: &keyframe.layer_time,
            source_time: &keyframe.source_time,
            interpolation: keyframe.interpolation,
        }
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "T: Deserialize<'de>"))]
struct CompositionArtifactWire<T> {
    schema_revision: u32,
    root_id: u64,
    content_revision: u64,
    #[serde(deserialize_with = "deserialize_bounded_compositions")]
    compositions: Vec<CompositionWire<T>>,
}

impl<T> CompositionArtifactWire<T> {
    fn into_artifact(self) -> Result<CompositionArtifact<T>> {
        if self.schema_revision != COMPOSITION_ARTIFACT_SCHEMA_REVISION {
            return Err(composition_error(
                ErrorCategory::Unsupported,
                Recoverability::UserCorrectable,
                "deserialize_composition_artifact",
                "unsupported_schema_revision",
                "composition artifact schema revision is not supported",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "deserialize_composition_artifact")
                    .with_field("schema_revision", self.schema_revision.to_string())
                    .with_field(
                        "supported_schema_revision",
                        COMPOSITION_ARTIFACT_SCHEMA_REVISION.to_string(),
                    ),
            ));
        }
        if self.compositions.len() > MAX_COMPOSITIONS {
            return Err(resource_error(
                "deserialize_composition_artifact",
                "composition_limit_exceeded",
                "serialized artifact exceeds the supported composition bound",
            ));
        }
        let compositions = self
            .compositions
            .into_iter()
            .map(CompositionWire::into_composition)
            .collect::<Result<Vec<_>>>()?;
        CompositionArtifact::from_parts(
            CompositionId::from_raw(self.root_id),
            self.content_revision,
            compositions,
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "T: Deserialize<'de>"))]
struct CompositionWire<T> {
    id: u64,
    name: String,
    range: TimeRange,
    #[serde(deserialize_with = "deserialize_bounded_layers")]
    layers: Vec<CompositionLayerWire<T>>,
}

impl<T> CompositionWire<T> {
    fn into_composition(self) -> Result<Composition<T>> {
        let layers = self
            .layers
            .into_iter()
            .map(CompositionLayerWire::into_layer)
            .collect::<Result<Vec<_>>>()?;
        Composition::from_parts(
            CompositionId::from_raw(self.id),
            self.name,
            self.range,
            layers,
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields, bound(deserialize = "T: Deserialize<'de>"))]
struct CompositionLayerWire<T> {
    id: u64,
    name: String,
    active_range: TimeRange,
    parent_id: Option<u64>,
    source: LayerSourceWire<T>,
    time_remap: TimeRemapWire,
    collapse: PrecompositionCollapse,
    isolation: LayerIsolation,
}

impl<T> CompositionLayerWire<T> {
    fn into_layer(self) -> Result<CompositionLayer<T>> {
        let id = CompositionLayerId::from_raw(self.id);
        let source = match self.source {
            LayerSourceWire::Visual { payload } => LayerSource::Visual(payload),
            LayerSourceWire::Precomposition {
                composition_id,
                payload,
            } => LayerSource::Precomposition {
                composition_id: CompositionId::from_raw(composition_id),
                payload,
            },
        };
        CompositionLayer::from_parts(
            id,
            self.name,
            self.active_range,
            self.parent_id.map(CompositionLayerId::from_raw),
            source,
            self.time_remap.into_time_remap()?,
            self.collapse,
            self.isolation,
        )
    }
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum LayerSourceWire<T> {
    Visual { payload: T },
    Precomposition { composition_id: u64, payload: T },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TimeRemapWire {
    #[serde(deserialize_with = "deserialize_bounded_time_remap_keyframes")]
    keyframes: Vec<TimeRemapKeyframeWire>,
}

impl TimeRemapWire {
    fn into_time_remap(self) -> Result<TimeRemap> {
        TimeRemap::new(
            self.keyframes
                .into_iter()
                .map(TimeRemapKeyframeWire::into_keyframe),
        )
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TimeRemapKeyframeWire {
    layer_time: RationalTime,
    source_time: RationalTime,
    interpolation: TimeRemapInterpolation,
}

impl TimeRemapKeyframeWire {
    fn into_keyframe(self) -> TimeRemapKeyframe {
        TimeRemapKeyframe::new(self.layer_time, self.source_time, self.interpolation)
    }
}

fn deserialize_bounded_compositions<'de, D, T>(
    deserializer: D,
) -> std::result::Result<Vec<CompositionWire<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    deserializer.deserialize_seq(BoundedVecVisitor::new(
        MAX_COMPOSITIONS,
        "visual compositions",
    ))
}

fn deserialize_bounded_layers<'de, D, T>(
    deserializer: D,
) -> std::result::Result<Vec<CompositionLayerWire<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    deserializer.deserialize_seq(BoundedVecVisitor::new(
        MAX_LAYERS_PER_COMPOSITION,
        "composition layers",
    ))
}

fn deserialize_bounded_time_remap_keyframes<'de, D>(
    deserializer: D,
) -> std::result::Result<Vec<TimeRemapKeyframeWire>, D::Error>
where
    D: Deserializer<'de>,
{
    deserializer.deserialize_seq(BoundedVecVisitor::new(
        MAX_TIME_REMAP_KEYFRAMES,
        "time-remap keyframes",
    ))
}

struct BoundedVecVisitor<T> {
    limit: usize,
    description: &'static str,
    marker: PhantomData<fn() -> T>,
}

impl<T> BoundedVecVisitor<T> {
    const fn new(limit: usize, description: &'static str) -> Self {
        Self {
            limit,
            description,
            marker: PhantomData,
        }
    }
}

impl<'de, T> Visitor<'de> for BoundedVecVisitor<T>
where
    T: Deserialize<'de>,
{
    type Value = Vec<T>;

    fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "at most {} {}", self.limit, self.description)
    }

    fn visit_seq<A>(self, mut sequence: A) -> std::result::Result<Self::Value, A::Error>
    where
        A: SeqAccess<'de>,
    {
        if sequence.size_hint().is_some_and(|hint| hint > self.limit) {
            return Err(A::Error::custom(format_args!(
                "{} exceed the supported bound of {}",
                self.description, self.limit
            )));
        }
        let mut values = Vec::with_capacity(sequence.size_hint().unwrap_or(0).min(self.limit));
        while let Some(value) = sequence.next_element()? {
            if values.len() == self.limit {
                return Err(A::Error::custom(format_args!(
                    "{} exceed the supported bound of {}",
                    self.description, self.limit
                )));
            }
            values.push(value);
        }
        Ok(values)
    }
}

fn rounded_divide(
    numerator: i128,
    denominator: i128,
    rounding: TimeRounding,
    operation: &'static str,
) -> Result<i128> {
    debug_assert!(denominator > 0);
    let quotient = numerator / denominator;
    let remainder = numerator % denominator;
    match rounding {
        TimeRounding::Exact => {
            if remainder != 0 {
                return Err(invalid_error(
                    operation,
                    "inexact_time_remap",
                    "time remap cannot represent the sampled source coordinate exactly",
                ));
            }
            Ok(quotient)
        }
        TimeRounding::Floor if remainder < 0 => Ok(quotient - 1),
        TimeRounding::Floor | TimeRounding::TowardZero => Ok(quotient),
        TimeRounding::Ceil if remainder > 0 => Ok(quotient + 1),
        TimeRounding::Ceil => Ok(quotient),
        TimeRounding::NearestTiesEven => {
            let doubled = remainder.abs() * 2;
            Ok(match doubled.cmp(&denominator) {
                Ordering::Less => quotient,
                Ordering::Greater => quotient + numerator.signum(),
                Ordering::Equal if quotient % 2 == 0 => quotient,
                Ordering::Equal => quotient + numerator.signum(),
            })
        }
        _ => Err(composition_error(
            ErrorCategory::Unsupported,
            Recoverability::UserCorrectable,
            operation,
            "unsupported_time_rounding",
            "time rounding policy is not supported by this composition revision",
        )),
    }
}

fn indexed_error(
    operation: &'static str,
    reason: &'static str,
    message: &'static str,
    index: usize,
) -> Error {
    invalid_error(operation, reason, message).with_context(
        ErrorContext::new(COMPONENT, operation).with_field("keyframe_index", index.to_string()),
    )
}

fn indexed_layer_error(
    operation: &'static str,
    layer_id: CompositionLayerId,
    reason: &'static str,
    message: &'static str,
    index: usize,
) -> Error {
    layer_error(operation, layer_id, reason, message).with_context(
        ErrorContext::new(COMPONENT, operation).with_field("keyframe_index", index.to_string()),
    )
}

fn layer_error(
    operation: &'static str,
    layer_id: CompositionLayerId,
    reason: &'static str,
    message: &'static str,
) -> Error {
    invalid_error(operation, reason, message).with_context(
        ErrorContext::new(COMPONENT, operation).with_field("layer_id", layer_id.to_string()),
    )
}

fn composition_id_error(
    operation: &'static str,
    composition_id: CompositionId,
    reason: &'static str,
    message: &'static str,
) -> Error {
    invalid_error(operation, reason, message).with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("composition_id", composition_id.to_string()),
    )
}

fn invalid_error(operation: &'static str, reason: &'static str, message: &'static str) -> Error {
    composition_error(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        operation,
        reason,
        message,
    )
}

fn resource_error(operation: &'static str, reason: &'static str, message: &'static str) -> Error {
    composition_error(
        ErrorCategory::ResourceExhausted,
        Recoverability::UserCorrectable,
        operation,
        reason,
        message,
    )
}

fn composition_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    reason: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation).with_field("reason", reason))
}
