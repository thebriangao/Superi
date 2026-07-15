use std::collections::BTreeMap;

use superi_core::color_space::ColorSpace;
use superi_core::error::Error;
use superi_core::geometry::PixelBounds;
use superi_core::pixel::{AlphaMode, PixelFormat};
use superi_core::time::{RationalTime, TimeRange, TimeRounding, Timebase};
use superi_effects::authoring::{EffectInstanceBindings, EffectParameterBinding};
use superi_effects::composition::{
    Composition, CompositionArtifact, CompositionId, CompositionLayer, CompositionLayerId,
    LayerIsolation, PrecompositionCollapse, TimeRemap, TimeRemapInterpolation, TimeRemapKeyframe,
    MAX_COMPOSITIONS,
};
use superi_effects::keyframe::{
    AnimationCurve, AnimationValue, Easing, Interpolation, Keyframe, KeyframeTiming,
};
use superi_effects::reference::ReferenceSampling;
use superi_effects::spatial::{
    render_spatial_reference, spatial_node_definition, CameraProjection, DepthOrdering, LayerSpace,
    LayerTransform, MotionBlur, SampledLightKind, SampledProjection, SpatialCamera,
    SpatialCompositionArtifact, SpatialLayer, SpatialLight, SpatialScene, SpatialSourceId,
    MAX_MOTION_SAMPLES, MAX_SPATIAL_LIGHTS,
};
use superi_graph::ids::{GraphId, NodeId, ParameterId, PortId};
use superi_graph::mutate::{EditableGraph, GraphMutation, GraphTransaction, InstancePort};
use superi_graph::node::{ParameterName, PortCardinality, PortName, TimeBehavior};
use superi_graph::serialize::{deserialize_graph, serialize_graph};
use superi_graph::value::GraphValue;
use superi_image::limits::ImageLimits;
use superi_image::value::{Image, ImageDescriptor, ImageSamples};

fn clock() -> Timebase {
    Timebase::integer(24).unwrap()
}

fn at(value: i64) -> RationalTime {
    RationalTime::new(value, clock())
}

fn range(start: i64, end: i64) -> TimeRange {
    TimeRange::from_start_end(at(start), at(end)).unwrap()
}

fn key(time: i64, components: &[f64]) -> Keyframe {
    Keyframe::new(
        KeyframeTiming::Fixed(at(time)),
        AnimationValue::new(components.iter().copied()).unwrap(),
        Interpolation::Linear,
        Easing::Linear,
        None,
        None,
    )
    .unwrap()
}

fn curve(components: &[f64]) -> AnimationCurve {
    AnimationCurve::new(clock(), [key(0, components)], None).unwrap()
}

fn animated(from: &[f64], to: &[f64]) -> AnimationCurve {
    AnimationCurve::new(clock(), [key(0, from), key(10, to)], None).unwrap()
}

fn direct_remap() -> TimeRemap {
    TimeRemap::new([
        TimeRemapKeyframe::new(at(0), at(0), TimeRemapInterpolation::Linear),
        TimeRemapKeyframe::new(at(10), at(10), TimeRemapInterpolation::Hold),
    ])
    .unwrap()
}

fn short_remap() -> TimeRemap {
    TimeRemap::new([
        TimeRemapKeyframe::new(at(0), at(0), TimeRemapInterpolation::Linear),
        TimeRemapKeyframe::new(at(2), at(2), TimeRemapInterpolation::Hold),
    ])
    .unwrap()
}

fn mapped_remap(layer_end: i64, source_end: i64) -> TimeRemap {
    TimeRemap::new([
        TimeRemapKeyframe::new(at(0), at(0), TimeRemapInterpolation::Linear),
        TimeRemapKeyframe::new(at(layer_end), at(source_end), TimeRemapInterpolation::Hold),
    ])
    .unwrap()
}

fn static_layer(source_id: u64, space: LayerSpace, position: [f64; 3]) -> SpatialLayer {
    SpatialLayer::new(
        SpatialSourceId::from_raw(source_id),
        space,
        LayerTransform::new(
            curve(&[0.5, 0.5, 0.0]),
            curve(&position),
            curve(&[0.0, 0.0, 0.0]),
            curve(&[1.0, 1.0, 1.0]),
            curve(&[1.0]),
        )
        .unwrap(),
        ReferenceSampling::Nearest,
    )
    .unwrap()
}

fn orthographic_camera() -> SpatialCamera {
    SpatialCamera::new(
        curve(&[0.0, 0.0, 10.0]),
        curve(&[0.0, 0.0, 0.0]),
        CameraProjection::orthographic(curve(&[1.0])).unwrap(),
        curve(&[0.1, 100.0]),
    )
    .unwrap()
}

fn render_artifact(
    layers: impl IntoIterator<Item = SpatialLayer>,
    depth_ordering: DepthOrdering,
    motion_blur: MotionBlur,
) -> SpatialCompositionArtifact {
    let composition_id = CompositionId::from_raw(40);
    let layers = layers
        .into_iter()
        .enumerate()
        .map(|(index, layer)| {
            CompositionLayer::visual(
                CompositionLayerId::from_raw(index as u64 + 1),
                format!("Layer {index}"),
                range(0, 3),
                layer,
                short_remap(),
            )
            .unwrap()
        })
        .collect::<Vec<_>>();
    let composition = Composition::new(composition_id, "Render root", range(0, 3), layers).unwrap();
    let scene = SpatialScene::new(
        composition_id,
        orthographic_camera(),
        [SpatialLight::ambient(curve(&[1.0, 1.0, 1.0]), curve(&[1.0])).unwrap()],
        depth_ordering,
        motion_blur,
    )
    .unwrap();
    SpatialCompositionArtifact::new(
        CompositionArtifact::new(composition_id, [composition]).unwrap(),
        [scene],
    )
    .unwrap()
}

fn bounds(width: u32, height: u32) -> PixelBounds {
    PixelBounds::from_origin_size(0, 0, width, height).unwrap()
}

fn image(pixel: [f32; 4]) -> Image {
    let region = bounds(1, 1);
    let descriptor = ImageDescriptor::new(
        region,
        region,
        PixelFormat::Rgba32Float,
        ColorSpace::ACESCG,
        AlphaMode::Premultiplied,
    )
    .unwrap();
    Image::new(descriptor, ImageSamples::from_f32(pixel)).unwrap()
}

fn pixels(image: &Image) -> Vec<[f32; 4]> {
    (0..image.samples().len() / 4)
        .map(|index| {
            std::array::from_fn(|channel| image.samples().float_value(index * 4 + channel).unwrap())
        })
        .collect()
}

fn assert_pixel_close(actual: [f32; 4], expected: [f32; 4]) {
    for channel in 0..4 {
        assert!(
            (actual[channel] - expected[channel]).abs() < 1.0e-6,
            "channel {channel}: expected {}, got {}",
            expected[channel],
            actual[channel]
        );
    }
}

fn reason(error: &Error) -> Option<&str> {
    error
        .contexts()
        .iter()
        .find_map(|context| context.field("reason"))
}

fn transform(position: AnimationCurve, space: LayerSpace) -> SpatialLayer {
    SpatialLayer::new(
        SpatialSourceId::from_raw(match space {
            LayerSpace::TwoDimensional => 10,
            LayerSpace::ThreeDimensional => 20,
        }),
        space,
        LayerTransform::new(
            curve(&[0.0, 0.0, 0.0]),
            position,
            curve(&[0.0, 0.0, 0.0]),
            curve(&[1.0, 1.0, 1.0]),
            curve(&[1.0]),
        )
        .unwrap(),
        ReferenceSampling::Bilinear,
    )
    .unwrap()
}

fn artifact() -> SpatialCompositionArtifact {
    let root_id = CompositionId::from_raw(1);
    let composition = Composition::new(
        root_id,
        "Spatial root",
        range(0, 11),
        [
            CompositionLayer::visual(
                CompositionLayerId::from_raw(1),
                "2D layer",
                range(0, 11),
                transform(
                    animated(&[0.0, 1.0, 2.0], &[10.0, 11.0, 12.0]),
                    LayerSpace::TwoDimensional,
                ),
                direct_remap(),
            )
            .unwrap(),
            CompositionLayer::visual(
                CompositionLayerId::from_raw(2),
                "3D layer",
                range(0, 11),
                transform(curve(&[2.0, 3.0, -4.0]), LayerSpace::ThreeDimensional),
                direct_remap(),
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let compositions = CompositionArtifact::new(root_id, [composition]).unwrap();
    let camera = SpatialCamera::new(
        curve(&[0.0, 0.0, 10.0]),
        curve(&[0.0, 0.0, 0.0]),
        CameraProjection::Perspective {
            vertical_field_of_view_degrees: animated(&[45.0], &[55.0]),
        },
        curve(&[0.1, 1_000.0]),
    )
    .unwrap();
    let scene = SpatialScene::new(
        root_id,
        camera,
        [SpatialLight::ambient(curve(&[1.0, 0.5, 0.25]), animated(&[0.5], &[1.5])).unwrap()],
        DepthOrdering::LayerStack,
        MotionBlur::new(-1, 1, 3).unwrap(),
    )
    .unwrap();
    SpatialCompositionArtifact::new(compositions, [scene]).unwrap()
}

#[test]
fn spatial_state_is_precise_editable_retimable_and_strictly_reloadable() {
    let artifact = artifact();
    let frame = artifact.sample(at(5), TimeRounding::Exact).unwrap();

    assert_eq!(frame.layers().len(), 2);
    assert_eq!(frame.layers()[0].space(), LayerSpace::TwoDimensional);
    assert_eq!(frame.layers()[0].world_matrix()[0][3], 5.0);
    assert_eq!(frame.layers()[0].world_matrix()[1][3], 6.0);
    assert_eq!(frame.layers()[0].world_matrix()[2][3], 7.0);
    assert_eq!(frame.layers()[1].space(), LayerSpace::ThreeDimensional);
    assert_eq!(frame.layers()[1].world_matrix()[2][3], -4.0);
    assert_eq!(frame.camera().position(), [0.0, 0.0, 10.0]);
    assert_eq!(frame.lights()[0].intensity(), 1.0);
    assert_eq!(frame.motion_sample_offsets(), [-1, 0, 1]);

    let document = serde_json::to_value(&artifact).unwrap();
    assert_eq!(document["schema_revision"], 1);
    let loaded: SpatialCompositionArtifact = serde_json::from_value(document.clone()).unwrap();
    assert_eq!(loaded, artifact);
    assert_eq!(loaded.sample(at(5), TimeRounding::Exact).unwrap(), frame);

    let retimed = artifact.retimed(at(0), at(20)).unwrap();
    assert_eq!(
        retimed
            .sample(at(10), TimeRounding::Exact)
            .unwrap()
            .layers()[0]
            .world_matrix(),
        frame.layers()[0].world_matrix()
    );
    let alternate_clock = Timebase::integer(48).unwrap();
    let rescaled_retime = artifact
        .retimed(
            RationalTime::new(0, alternate_clock),
            RationalTime::new(40, alternate_clock),
        )
        .unwrap();
    assert_eq!(
        rescaled_retime
            .sample(at(10), TimeRounding::Exact)
            .unwrap()
            .layers()[0]
            .world_matrix(),
        frame.layers()[0].world_matrix()
    );

    let mut future = document.clone();
    future["schema_revision"] = 2.into();
    assert!(serde_json::from_value::<SpatialCompositionArtifact>(future).is_err());
    let mut unknown = document;
    unknown["future_control"] = true.into();
    assert!(serde_json::from_value::<SpatialCompositionArtifact>(unknown).is_err());
}

#[test]
fn spatial_definition_is_one_animatable_graph_contract_for_both_workflows() {
    let artifact = artifact();
    let definition = spatial_node_definition(artifact.clone()).unwrap();
    assert_eq!(
        definition.schema().behavior().time(),
        TimeBehavior::Unbounded
    );
    assert_eq!(
        definition
            .input(&PortName::new("layers").unwrap())
            .unwrap()
            .schema()
            .cardinality(),
        PortCardinality::Variadic
    );
    assert!(definition
        .parameter(&ParameterName::new("composition").unwrap())
        .unwrap()
        .schema()
        .is_animatable());

    let node = definition
        .instantiate(
            EffectInstanceBindings::new(
                [InstancePort::new(
                    PortId::from_raw(1),
                    PortName::new("layers").unwrap(),
                )],
                [InstancePort::new(
                    PortId::from_raw(2),
                    PortName::new("result").unwrap(),
                )],
                [EffectParameterBinding::new(
                    ParameterId::from_raw(3),
                    ParameterName::new("composition").unwrap(),
                )],
            ),
            [],
        )
        .unwrap();

    let sources = BTreeMap::from([
        (SpatialSourceId::from_raw(10), image([1.0, 0.0, 0.0, 1.0])),
        (SpatialSourceId::from_raw(20), image([0.0, 0.0, 1.0, 1.0])),
    ]);
    let expected_render = render_spatial_reference(
        &artifact,
        at(5),
        TimeRounding::Exact,
        &sources,
        bounds(5, 5),
        &ImageLimits::default(),
    )
    .unwrap();
    for (graph_raw, node_raw) in [(100_u128, 10_u128), (200, 20)] {
        let mut graph = EditableGraph::new(GraphId::from_raw(graph_raw));
        graph
            .apply(GraphTransaction::with_mutations(
                0,
                [GraphMutation::Add {
                    node_id: NodeId::from_raw(node_raw),
                    node: node.clone(),
                    position: 0,
                }],
            ))
            .unwrap();
        let bytes = serialize_graph(&graph.snapshot()).unwrap();
        let loaded = deserialize_graph::<GraphValue<SpatialCompositionArtifact>>(&bytes).unwrap();
        let value = loaded
            .graph()
            .snapshot()
            .node(NodeId::from_raw(node_raw))
            .unwrap()
            .parameter(ParameterId::from_raw(3))
            .unwrap()
            .value()
            .payload()
            .as_domain()
            .unwrap()
            .clone();
        assert_eq!(value, artifact);
        assert_eq!(
            value.sample(at(5), TimeRounding::Exact).unwrap(),
            artifact.sample(at(5), TimeRounding::Exact).unwrap()
        );
        assert_eq!(
            render_spatial_reference(
                &value,
                at(5),
                TimeRounding::Exact,
                &sources,
                bounds(5, 5),
                &ImageLimits::default(),
            )
            .unwrap(),
            expected_render
        );
        assert_eq!(serialize_graph(&loaded.graph().snapshot()).unwrap(), bytes);
    }
}

#[test]
fn camera_depth_reorders_real_two_and_three_dimensional_pixels() {
    let near_blue = static_layer(2, LayerSpace::ThreeDimensional, [0.0, 0.0, 2.0]);
    let far_red = static_layer(1, LayerSpace::TwoDimensional, [0.0, 0.0, 0.0]);
    let artifact = render_artifact(
        [near_blue, far_red],
        DepthOrdering::CameraDepth,
        MotionBlur::new(0, 0, 1).unwrap(),
    );
    let sampled = artifact.sample(at(1), TimeRounding::Exact).unwrap();
    assert_eq!(
        sampled.layers()[0].source_id(),
        SpatialSourceId::from_raw(1)
    );
    assert_eq!(
        sampled.layers()[1].source_id(),
        SpatialSourceId::from_raw(2)
    );
    assert!(sampled.layers()[0].camera_depth() > sampled.layers()[1].camera_depth());

    let sources = BTreeMap::from([
        (SpatialSourceId::from_raw(1), image([1.0, 0.0, 0.0, 1.0])),
        (SpatialSourceId::from_raw(2), image([0.0, 0.0, 1.0, 1.0])),
    ]);
    let rendered = render_spatial_reference(
        &artifact,
        at(1),
        TimeRounding::Exact,
        &sources,
        bounds(5, 1),
        &ImageLimits::default(),
    )
    .unwrap();
    assert_eq!(pixels(&rendered)[2], [0.0, 0.0, 1.0, 1.0]);
}

#[test]
fn lights_and_exact_shutter_samples_produce_bounded_motion_pixels() {
    let moving = SpatialLayer::new(
        SpatialSourceId::from_raw(7),
        LayerSpace::TwoDimensional,
        LayerTransform::new(
            curve(&[0.5, 0.5, 0.0]),
            AnimationCurve::new(
                clock(),
                [key(0, &[-1.0, 0.0, 0.0]), key(2, &[1.0, 0.0, 0.0])],
                None,
            )
            .unwrap(),
            curve(&[0.0, 0.0, 0.0]),
            curve(&[1.0, 1.0, 1.0]),
            curve(&[1.0]),
        )
        .unwrap(),
        ReferenceSampling::Nearest,
    )
    .unwrap();
    let artifact = render_artifact(
        [moving],
        DepthOrdering::LayerStack,
        MotionBlur::new(-1, 1, 3).unwrap(),
    );
    let sources = BTreeMap::from([(SpatialSourceId::from_raw(7), image([1.0, 0.0, 0.0, 1.0]))]);
    let rendered = render_spatial_reference(
        &artifact,
        at(1),
        TimeRounding::Exact,
        &sources,
        bounds(5, 1),
        &ImageLimits::default(),
    )
    .unwrap();
    let output = pixels(&rendered);
    for index in [1_usize, 2, 3] {
        assert!((output[index][0] - 1.0 / 3.0).abs() < 1.0e-6);
        assert!((output[index][3] - 1.0 / 3.0).abs() < 1.0e-6);
    }
    assert_eq!(output[0], [0.0; 4]);
    assert_eq!(output[4], [0.0; 4]);
}

#[test]
fn perspective_directional_and_point_lights_execute_real_pixels() {
    let root_id = CompositionId::from_raw(40);
    let artifact = render_artifact(
        [static_layer(
            1,
            LayerSpace::ThreeDimensional,
            [0.0, 0.0, 0.0],
        )],
        DepthOrdering::LayerStack,
        MotionBlur::new(0, 0, 1).unwrap(),
    );
    let camera = SpatialCamera::new(
        curve(&[0.0, 0.0, 10.0]),
        curve(&[0.0, 0.0, 0.0]),
        CameraProjection::perspective(curve(&[90.0])).unwrap(),
        curve(&[0.1, 100.0]),
    )
    .unwrap();
    let scene = SpatialScene::new(
        root_id,
        camera,
        [
            SpatialLight::directional(
                curve(&[0.0, 0.0, 0.0]),
                curve(&[0.5, 0.25, 0.125]),
                curve(&[1.0]),
            )
            .unwrap(),
            SpatialLight::point(
                curve(&[0.0, 0.0, 10.0]),
                curve(&[0.25, 0.5, 0.75]),
                curve(&[1.0]),
                curve(&[10.0]),
            )
            .unwrap(),
        ],
        DepthOrdering::LayerStack,
        MotionBlur::new(0, 0, 1).unwrap(),
    )
    .unwrap();
    let artifact = artifact.with_scene(scene).unwrap();
    let sampled = artifact.sample(at(1), TimeRounding::Exact).unwrap();
    assert!(matches!(
        sampled.camera().projection(),
        SampledProjection::Perspective {
            vertical_field_of_view_degrees: 90.0
        }
    ));
    assert!(matches!(
        sampled.lights()[0].kind(),
        SampledLightKind::Directional { .. }
    ));
    assert!(matches!(
        sampled.lights()[1].kind(),
        SampledLightKind::Point { .. }
    ));

    let sources = BTreeMap::from([(SpatialSourceId::from_raw(1), image([1.0, 1.0, 1.0, 1.0]))]);
    let rendered = render_spatial_reference(
        &artifact,
        at(1),
        TimeRounding::Exact,
        &sources,
        bounds(1, 1),
        &ImageLimits::default(),
    )
    .unwrap();
    assert_pixel_close(pixels(&rendered)[0], [0.625, 0.5, 0.5, 1.0]);
}

#[test]
fn nested_precomposition_and_parent_transforms_use_each_owning_clock() {
    let root_id = CompositionId::from_raw(100);
    let child_id = CompositionId::from_raw(200);
    let root_layer_id = CompositionLayerId::from_raw(1);
    let parent_id = CompositionLayerId::from_raw(2);
    let child_layer_id = CompositionLayerId::from_raw(3);
    let root_layer = CompositionLayer::precomposition(
        root_layer_id,
        "Child instance",
        range(0, 11),
        child_id,
        static_layer(60, LayerSpace::ThreeDimensional, [1.0, 0.0, 0.0]),
        mapped_remap(10, 20),
        PrecompositionCollapse::Collapse,
        LayerIsolation::PassThrough,
    )
    .unwrap();
    let parent = CompositionLayer::visual(
        parent_id,
        "Parent",
        range(0, 21),
        static_layer(70, LayerSpace::ThreeDimensional, [2.0, 0.0, 0.0]),
        mapped_remap(20, 20),
    )
    .unwrap();
    let child_position = AnimationCurve::new(
        clock(),
        [key(0, &[0.0, 0.0, 0.0]), key(20, &[10.0, 0.0, 0.0])],
        None,
    )
    .unwrap();
    let child = CompositionLayer::visual(
        child_layer_id,
        "Child",
        range(0, 21),
        transform(child_position, LayerSpace::ThreeDimensional),
        mapped_remap(20, 20),
    )
    .unwrap()
    .with_parent(Some(parent_id));
    let compositions = CompositionArtifact::new(
        root_id,
        [
            Composition::new(root_id, "Root", range(0, 11), [root_layer]).unwrap(),
            Composition::new(child_id, "Child", range(0, 21), [parent, child]).unwrap(),
        ],
    )
    .unwrap();
    let scenes = [root_id, child_id].map(|composition_id| {
        SpatialScene::new(
            composition_id,
            orthographic_camera(),
            [SpatialLight::ambient(curve(&[1.0; 3]), curve(&[1.0])).unwrap()],
            DepthOrdering::LayerStack,
            MotionBlur::new(0, 0, 1).unwrap(),
        )
        .unwrap()
    });
    let artifact = SpatialCompositionArtifact::new(compositions, scenes).unwrap();
    let sampled = artifact.sample(at(5), TimeRounding::Exact).unwrap();
    let layer = sampled
        .layers()
        .iter()
        .find(|layer| layer.source_id() == SpatialSourceId::from_raw(20))
        .unwrap();
    assert_eq!(layer.world_matrix()[0][3], 7.0);
    assert_eq!(layer.path().len(), 3);
    assert_eq!(
        layer
            .path()
            .iter()
            .map(|step| step.layer_id())
            .collect::<Vec<_>>(),
        [root_layer_id, parent_id, child_layer_id]
    );
    assert_eq!(
        layer
            .path()
            .iter()
            .map(|step| step.composition_time().value())
            .collect::<Vec<_>>(),
        [5, 10, 10]
    );
    assert_eq!(
        layer
            .path()
            .iter()
            .map(|step| step.source_time().value())
            .collect::<Vec<_>>(),
        [10, 10, 10]
    );
    assert_eq!(layer.path()[0].position(), [1.0, 0.0, 0.0]);
    assert_eq!(layer.path()[1].position(), [2.0, 0.0, 0.0]);
    assert_eq!(layer.path()[2].position(), [5.0, 0.0, 0.0]);
    assert_eq!(layer.path()[0].local_matrix()[0][3], 0.5);
    assert_eq!(layer.path()[1].local_matrix()[0][3], 1.5);
    assert_eq!(layer.path()[2].local_matrix()[0][3], 5.0);
    assert!(layer.path().iter().all(|step| step.opacity() == 1.0));
}

#[test]
fn invalid_curve_shape_and_scene_coverage_are_rejected() {
    let wrong_clock = Timebase::integer(30).unwrap();
    let wrong_position = AnimationCurve::new(
        wrong_clock,
        [Keyframe::new(
            KeyframeTiming::Fixed(RationalTime::new(0, wrong_clock)),
            AnimationValue::new([0.0, 0.0, 0.0]).unwrap(),
            Interpolation::Linear,
            Easing::Linear,
            None,
            None,
        )
        .unwrap()],
        None,
    )
    .unwrap();
    assert!(LayerTransform::new(
        curve(&[0.0; 3]),
        wrong_position,
        curve(&[0.0; 3]),
        curve(&[1.0; 3]),
        curve(&[1.0]),
    )
    .is_err());
    assert!(LayerTransform::new(
        curve(&[0.0; 3]),
        curve(&[0.0; 3]),
        curve(&[0.0; 3]),
        curve(&[1.0; 3]),
        curve(&[1.0, 1.0]),
    )
    .is_err());

    let document = serde_json::to_value(artifact()).unwrap();
    let mut duplicate = document.clone();
    duplicate["scenes"] = serde_json::Value::Array(vec![
        document["scenes"][0].clone(),
        document["scenes"][0].clone(),
    ]);
    assert!(serde_json::from_value::<SpatialCompositionArtifact>(duplicate).is_err());
    let mut absent = document;
    absent["scenes"] = serde_json::Value::Array(Vec::new());
    assert!(serde_json::from_value::<SpatialCompositionArtifact>(absent).is_err());

    let document = serde_json::to_value(artifact()).unwrap();
    let scene = document["scenes"][0].clone();
    let mut excessive_scenes = document.clone();
    excessive_scenes["scenes"] =
        serde_json::Value::Array(std::iter::repeat_n(scene, MAX_COMPOSITIONS + 1).collect());
    assert!(serde_json::from_value::<SpatialCompositionArtifact>(excessive_scenes).is_err());

    let light = document["scenes"][0]["lights"][0].clone();
    let mut excessive_lights = document;
    excessive_lights["scenes"][0]["lights"] =
        serde_json::Value::Array(std::iter::repeat_n(light, MAX_SPATIAL_LIGHTS + 1).collect());
    assert!(serde_json::from_value::<SpatialCompositionArtifact>(excessive_lights).is_err());
}

#[test]
fn invalid_scene_source_and_resource_state_fail_before_unbounded_work() {
    assert!(MotionBlur::new(0, 1, MAX_MOTION_SAMPLES + 1).is_err());
    assert!(MotionBlur::new(-1, 2, 3).is_err());

    let artifact = render_artifact(
        [static_layer(
            99,
            LayerSpace::TwoDimensional,
            [0.0, 0.0, 0.0],
        )],
        DepthOrdering::LayerStack,
        MotionBlur::new(0, 0, 1).unwrap(),
    );
    let error = render_spatial_reference(
        &artifact,
        at(1),
        TimeRounding::Exact,
        &BTreeMap::new(),
        bounds(5, 1),
        &ImageLimits::default(),
    )
    .unwrap_err();
    assert_eq!(reason(&error), Some("missing_spatial_source"));

    let sources = BTreeMap::from([(SpatialSourceId::from_raw(99), image([1.0, 1.0, 1.0, 1.0]))]);
    let limits = ImageLimits::new(4, 1, 1_024).unwrap();
    assert!(render_spatial_reference(
        &artifact,
        at(1),
        TimeRounding::Exact,
        &sources,
        bounds(5, 1),
        &limits,
    )
    .is_err());
}
