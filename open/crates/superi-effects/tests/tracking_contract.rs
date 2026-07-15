use std::str::FromStr;

use superi_core::color_space::ColorSpace;
use superi_core::geometry::{Matrix3, Point2, Rect};
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_core::time::Timebase;
use superi_effects::authoring::{
    EffectInstanceBindings, EffectMetadata, EffectNodeDefinition, EffectParameterBinding,
    EffectParameterDefinition, ParameterControl,
};
use superi_effects::tracking::{
    CameraIntrinsics, CameraLandmark, CameraPose, CpuTrackingSolver, FeatureId, TrackId,
    TrackedFeature, TrackingArtifact, TrackingFrame, TrackingMatrix3, TrackingModel,
    TrackingObservation, TrackingPoint, TrackingPoint3, TrackingRect, TrackingResult,
    TrackingSample, TrackingSelection, TrackingSolver, TrackingTrack,
};
use superi_graph::ids::{GraphId, NodeId, ParameterId};
use superi_graph::mutate::{
    EditableGraph, EditableNode, GraphMutation, GraphTransaction, TypedParameterValue,
};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchemaId, NodeTypeId,
    ParameterName, ParameterSchema, RoiBehavior, TimeBehavior, ValueTypeId,
};
use superi_graph::serialize::{deserialize_graph, serialize_graph};
use superi_graph::value::GraphValue;

fn patterned_frame(frame: i64, shift_x: f64, shift_y: f64) -> TrackingFrame {
    let width = 64;
    let height = 64;
    let mut luma = Vec::with_capacity(width * height);
    for y in 0..height {
        for x in 0..width {
            let source_x = x as f64 - shift_x;
            let source_y = y as f64 - shift_y;
            let value = 0.45
                + 0.18 * (source_x * 0.31).sin()
                + 0.16 * (source_y * 0.27).cos()
                + 0.12 * ((source_x + source_y) * 0.19).sin();
            luma.push(value as f32);
        }
    }
    TrackingFrame::new(frame, width as u32, height as u32, luma).unwrap()
}

#[test]
fn point_tracking_flows_through_revisioned_editable_state() {
    let id = TrackId::from_raw(7);
    let source_point = TrackingPoint::new(31.0, 29.0).unwrap();
    let track = TrackingTrack::new(
        id,
        0,
        TrackingSelection::point(TrackedFeature::new(11, source_point)),
    )
    .unwrap();
    let artifact = TrackingArtifact::new(Timebase::integer(24).unwrap(), [track]).unwrap();

    let request = artifact.solve_request(id, 1).unwrap();
    assert_eq!(request.source().frame(), 0);
    assert_eq!(request.target_frame(), 1);

    let source = patterned_frame(0, 0.0, 0.0);
    let target = patterned_frame(1, 1.25, -0.75);
    let result = CpuTrackingSolver::default()
        .solve(&request, &source, &target)
        .unwrap();
    let updated = artifact.apply_solver_result(result).unwrap();
    assert_eq!(updated.revision(), 1);

    let resolved = updated.resolved_sample(id, 1).unwrap();
    assert_eq!(resolved.source().code(), "solver");
    let TrackingModel::Point { position } = resolved.sample().model() else {
        panic!("point selection must produce a point model");
    };
    assert!((position.x() - 32.25).abs() < 0.2);
    assert!((position.y() - 28.25).abs() < 0.2);
    assert_eq!(resolved.sample().observations().len(), 1);
    assert!(resolved.sample().observations()[0].confidence() > 0.5);
}

fn point_sample(frame: i64, x: f64, y: f64) -> TrackingSample {
    let position = TrackingPoint::new(x, y).unwrap();
    TrackingSample::new(
        frame,
        TrackingModel::Point { position },
        [TrackingObservation::new(FeatureId::from_raw(11), position, 1.0).unwrap()],
    )
    .unwrap()
}

#[test]
fn manual_corrections_drive_temporal_sources_and_fence_stale_results() {
    let id = TrackId::from_raw(7);
    let track = TrackingTrack::new(
        id,
        0,
        TrackingSelection::point(TrackedFeature::new(
            11,
            TrackingPoint::new(31.0, 29.0).unwrap(),
        )),
    )
    .unwrap();
    let solver = CpuTrackingSolver::default();
    let source = patterned_frame(0, 0.0, 0.0);
    let artifact = TrackingArtifact::new(Timebase::integer(24).unwrap(), [track]).unwrap();

    let backward = solver
        .solve(
            &artifact.solve_request(id, -1).unwrap(),
            &source,
            &patterned_frame(-1, -0.5, 0.25),
        )
        .unwrap();
    let artifact = artifact.apply_solver_result(backward).unwrap();
    let forward = solver
        .solve(
            &artifact.solve_request(id, 1).unwrap(),
            &source,
            &patterned_frame(1, 0.75, 0.5),
        )
        .unwrap();
    let artifact = artifact.apply_solver_result(forward).unwrap();
    assert_eq!(artifact.track(id).unwrap().derived_samples().len(), 2);

    let corrected = artifact
        .with_correction(id, point_sample(2, 34.0, 30.0))
        .unwrap();
    assert!(corrected.resolved_sample(id, -1).is_some());
    assert!(corrected.resolved_sample(id, 1).is_none());
    assert_eq!(
        corrected.resolved_sample(id, 2).unwrap().source().code(),
        "manual_correction"
    );
    assert_eq!(corrected.solve_request(id, 3).unwrap().source().frame(), 2);

    let pending = solver
        .solve(
            &corrected.solve_request(id, 3).unwrap(),
            &patterned_frame(2, 3.0, 1.0),
            &patterned_frame(3, 3.5, 1.25),
        )
        .unwrap();
    let edited = corrected
        .with_correction(id, point_sample(4, 35.0, 30.5))
        .unwrap();
    assert_eq!(
        edited.apply_solver_result(pending).unwrap_err().category(),
        superi_core::error::ErrorCategory::Conflict
    );

    let reopened = edited.without_correction(id, 4).unwrap();
    assert_eq!(reopened.track(id).unwrap().corrections().len(), 1);
    assert_eq!(reopened.revision(), edited.revision() + 1);
}

fn feature_frame(
    frame: i64,
    width: u32,
    height: u32,
    features: &[(u64, TrackingPoint)],
) -> TrackingFrame {
    let mut luma = Vec::with_capacity(width as usize * height as usize);
    for y in 0..height {
        for x in 0..width {
            let mut value = 0.04;
            for (id, position) in features {
                let dx = f64::from(x) - position.x();
                let dy = f64::from(y) - position.y();
                if dx.abs() <= 6.0 && dy.abs() <= 6.0 {
                    let envelope = (-(dx * dx + dy * dy) / 18.0).exp();
                    let seed = *id as f64 * 0.173;
                    value += envelope
                        * (0.55
                            + 0.16 * (dx * 0.83 + seed).sin()
                            + 0.12 * (dy * 0.71 - seed).cos());
                }
            }
            luma.push(value as f32);
        }
    }
    TrackingFrame::new(frame, width, height, luma).unwrap()
}

#[test]
fn planar_solver_tracks_features_and_publishes_a_region_homography() {
    let id = TrackId::from_raw(20);
    let source_features = [
        (1, TrackingPoint::new(25.0, 24.0).unwrap()),
        (2, TrackingPoint::new(55.0, 22.0).unwrap()),
        (3, TrackingPoint::new(83.0, 27.0).unwrap()),
        (4, TrackingPoint::new(28.0, 61.0).unwrap()),
        (5, TrackingPoint::new(58.0, 66.0).unwrap()),
        (6, TrackingPoint::new(86.0, 60.0).unwrap()),
    ];
    let shift = (1.2, -0.8);
    let mut target_features = source_features
        .iter()
        .map(|(feature_id, point)| {
            (
                *feature_id,
                TrackingPoint::new(point.x() + shift.0, point.y() + shift.1).unwrap(),
            )
        })
        .collect::<Vec<_>>();
    target_features[5].1 =
        TrackingPoint::new(target_features[5].1.x() + 4.0, target_features[5].1.y()).unwrap();
    let selection = TrackingSelection::planar(
        TrackingRect::new(
            TrackingPoint::new(18.0, 16.0).unwrap(),
            TrackingPoint::new(94.0, 74.0).unwrap(),
        )
        .unwrap(),
        source_features
            .iter()
            .map(|(feature_id, point)| TrackedFeature::new(*feature_id, *point)),
    )
    .unwrap();
    let artifact = TrackingArtifact::new(
        Timebase::integer(24).unwrap(),
        [TrackingTrack::new(id, 0, selection).unwrap()],
    )
    .unwrap();
    let result = CpuTrackingSolver::default()
        .solve(
            &artifact.solve_request(id, 1).unwrap(),
            &feature_frame(0, 112, 88, &source_features),
            &feature_frame(1, 112, 88, &target_features),
        )
        .unwrap();
    let solved = artifact.apply_solver_result(result).unwrap();
    let TrackingModel::Planar { homography, region } =
        solved.resolved_sample(id, 1).unwrap().sample().model()
    else {
        panic!("planar selection must produce a planar model");
    };
    let matrix = homography.values();
    assert!((matrix[2] - shift.0).abs() < 0.25);
    assert!((matrix[5] - shift.1).abs() < 0.25);
    assert!((region.min().x() - (18.0 + shift.0)).abs() < 0.25);
    assert_eq!(
        solved
            .resolved_sample(id, 1)
            .unwrap()
            .sample()
            .observations()
            .len(),
        6
    );
}

#[test]
fn object_solver_publishes_similarity_motion_and_transformed_bounds() {
    let id = TrackId::from_raw(30);
    let source_features = [
        (1, TrackingPoint::new(34.0, 30.0).unwrap()),
        (2, TrackingPoint::new(76.0, 31.0).unwrap()),
        (3, TrackingPoint::new(35.0, 70.0).unwrap()),
        (4, TrackingPoint::new(75.0, 69.0).unwrap()),
        (5, TrackingPoint::new(55.0, 49.0).unwrap()),
    ];
    let angle = 0.025_f64;
    let scale = 1.012_f64;
    let a = scale * angle.cos();
    let b = scale * angle.sin();
    let translation = (1.4, -1.1);
    let transform = |point: TrackingPoint| {
        TrackingPoint::new(
            a * point.x() - b * point.y() + translation.0,
            b * point.x() + a * point.y() + translation.1,
        )
        .unwrap()
    };
    let target_features = source_features
        .iter()
        .map(|(feature_id, point)| (*feature_id, transform(*point)))
        .collect::<Vec<_>>();
    let bounds = TrackingRect::new(
        TrackingPoint::new(28.0, 24.0).unwrap(),
        TrackingPoint::new(82.0, 76.0).unwrap(),
    )
    .unwrap();
    let selection = TrackingSelection::object(
        bounds,
        source_features
            .iter()
            .map(|(feature_id, point)| TrackedFeature::new(*feature_id, *point)),
    )
    .unwrap();
    let artifact = TrackingArtifact::new(
        Timebase::integer(24).unwrap(),
        [TrackingTrack::new(id, 0, selection).unwrap()],
    )
    .unwrap();
    let result = CpuTrackingSolver::default()
        .solve(
            &artifact.solve_request(id, 1).unwrap(),
            &feature_frame(0, 112, 96, &source_features),
            &feature_frame(1, 112, 96, &target_features),
        )
        .unwrap();
    let solved = artifact.apply_solver_result(result).unwrap();
    let TrackingModel::Object { transform, region } =
        solved.resolved_sample(id, 1).unwrap().sample().model()
    else {
        panic!("object selection must produce an object model");
    };
    let matrix = transform.values();
    assert!((matrix[0] - a).abs() < 0.02);
    assert!((matrix[3] - b).abs() < 0.02);
    assert!((matrix[2] - translation.0).abs() < 0.35);
    assert!(region.width() > bounds.width());
}

fn project(point: TrackingPoint3, pose: CameraPose, intrinsics: CameraIntrinsics) -> TrackingPoint {
    let translation = pose.translation();
    let x = point.x() + translation[0];
    let y = point.y() + translation[1];
    let z = point.z() + translation[2];
    TrackingPoint::new(
        intrinsics.focal_x() * x / z + intrinsics.principal_x(),
        intrinsics.focal_y() * y / z + intrinsics.principal_y(),
    )
    .unwrap()
}

#[test]
fn camera_solver_refines_calibrated_pose_from_known_landmarks() {
    let id = TrackId::from_raw(40);
    let intrinsics = CameraIntrinsics::new(120.0, 118.0, 80.0, 60.0).unwrap();
    let source_pose = CameraPose::identity();
    let target_pose = CameraPose::new([0.0, 0.0, 0.0], [0.045, -0.03, 0.02]).unwrap();
    let world = [
        TrackingPoint3::new(-1.2, -0.8, 5.0).unwrap(),
        TrackingPoint3::new(1.1, -0.7, 5.6).unwrap(),
        TrackingPoint3::new(-1.0, 0.9, 6.2).unwrap(),
        TrackingPoint3::new(1.2, 0.8, 6.8).unwrap(),
        TrackingPoint3::new(-0.4, -0.2, 7.4).unwrap(),
        TrackingPoint3::new(0.5, 0.3, 7.9).unwrap(),
        TrackingPoint3::new(-0.7, 0.5, 8.5).unwrap(),
        TrackingPoint3::new(0.8, -0.4, 9.0).unwrap(),
    ];
    let source_features = world
        .iter()
        .enumerate()
        .map(|(index, point)| ((index + 1) as u64, project(*point, source_pose, intrinsics)))
        .collect::<Vec<_>>();
    let target_features = world
        .iter()
        .enumerate()
        .map(|(index, point)| ((index + 1) as u64, project(*point, target_pose, intrinsics)))
        .collect::<Vec<_>>();
    let landmarks = world.iter().enumerate().map(|(index, point)| {
        CameraLandmark::new((index + 1) as u64, *point, source_features[index].1)
    });
    let selection = TrackingSelection::camera(intrinsics, source_pose, landmarks).unwrap();
    let artifact = TrackingArtifact::new(
        Timebase::integer(24).unwrap(),
        [TrackingTrack::new(id, 0, selection).unwrap()],
    )
    .unwrap();
    let result = CpuTrackingSolver::default()
        .solve(
            &artifact.solve_request(id, 1).unwrap(),
            &feature_frame(0, 160, 120, &source_features),
            &feature_frame(1, 160, 120, &target_features),
        )
        .unwrap();
    let solved = artifact.apply_solver_result(result).unwrap();
    let TrackingModel::Camera { pose } = solved.resolved_sample(id, 1).unwrap().sample().model()
    else {
        panic!("camera selection must produce a camera model");
    };
    let translation = pose.translation();
    assert!((translation[0] - 0.045).abs() < 0.02);
    assert!((translation[1] + 0.03).abs() < 0.02);
    assert!((translation[2] - 0.02).abs() < 0.04);
}

fn all_kind_artifact() -> TrackingArtifact {
    let planar_features = [
        TrackedFeature::new(21, TrackingPoint::new(20.0, 20.0).unwrap()),
        TrackedFeature::new(22, TrackingPoint::new(60.0, 20.0).unwrap()),
        TrackedFeature::new(23, TrackingPoint::new(20.0, 60.0).unwrap()),
        TrackedFeature::new(24, TrackingPoint::new(60.0, 60.0).unwrap()),
    ];
    let object_features = [
        TrackedFeature::new(31, TrackingPoint::new(30.0, 30.0).unwrap()),
        TrackedFeature::new(32, TrackingPoint::new(70.0, 30.0).unwrap()),
        TrackedFeature::new(33, TrackingPoint::new(30.0, 70.0).unwrap()),
    ];
    let intrinsics = CameraIntrinsics::new(100.0, 100.0, 64.0, 48.0).unwrap();
    let world = [
        TrackingPoint3::new(-1.0, -1.0, 5.0).unwrap(),
        TrackingPoint3::new(1.0, -1.0, 5.5).unwrap(),
        TrackingPoint3::new(-1.0, 1.0, 6.0).unwrap(),
        TrackingPoint3::new(1.0, 1.0, 6.5).unwrap(),
        TrackingPoint3::new(-0.5, 0.2, 7.0).unwrap(),
        TrackingPoint3::new(0.6, -0.3, 7.8).unwrap(),
    ];
    let camera = world.iter().enumerate().map(|(index, point)| {
        CameraLandmark::new(
            41 + index as u64,
            *point,
            project(*point, CameraPose::identity(), intrinsics),
        )
    });
    let region = TrackingRect::new(
        TrackingPoint::new(15.0, 15.0).unwrap(),
        TrackingPoint::new(75.0, 75.0).unwrap(),
    )
    .unwrap();
    TrackingArtifact::new(
        Timebase::integer(24).unwrap(),
        [
            TrackingTrack::new(
                TrackId::from_raw(1),
                0,
                TrackingSelection::point(TrackedFeature::new(
                    11,
                    TrackingPoint::new(31.0, 29.0).unwrap(),
                )),
            )
            .unwrap(),
            TrackingTrack::new(
                TrackId::from_raw(2),
                0,
                TrackingSelection::planar(region, planar_features).unwrap(),
            )
            .unwrap(),
            TrackingTrack::new(
                TrackId::from_raw(3),
                0,
                TrackingSelection::object(region, object_features).unwrap(),
            )
            .unwrap(),
            TrackingTrack::new(
                TrackId::from_raw(4),
                0,
                TrackingSelection::camera(intrinsics, CameraPose::identity(), camera).unwrap(),
            )
            .unwrap(),
        ],
    )
    .unwrap()
    .with_correction(TrackId::from_raw(1), point_sample(2, 33.0, 30.0))
    .unwrap()
}

#[test]
fn tracking_wire_is_strict_bounded_and_reconstructs_checked_state() {
    let artifact = all_kind_artifact();
    let document = serde_json::to_value(&artifact).unwrap();
    assert_eq!(
        serde_json::from_value::<TrackingArtifact>(document.clone()).unwrap(),
        artifact
    );

    let mut future = document.clone();
    future["schema_revision"] = serde_json::json!(2);
    assert!(serde_json::from_value::<TrackingArtifact>(future).is_err());

    let mut unknown = document.clone();
    unknown["unexpected"] = serde_json::json!(true);
    assert!(serde_json::from_value::<TrackingArtifact>(unknown).is_err());

    let mut oversized = document.clone();
    let feature = oversized["tracks"][1]["selection"]["features"][0].clone();
    oversized["tracks"][1]["selection"]["features"] = serde_json::Value::Array(vec![feature; 257]);
    assert!(serde_json::from_value::<TrackingArtifact>(oversized).is_err());

    let mut nonfinite = document;
    nonfinite["tracks"][0]["reference"]["observations"][0]["confidence"] =
        serde_json::json!(f64::INFINITY.to_bits());
    assert!(serde_json::from_value::<TrackingArtifact>(nonfinite).is_err());
}

fn tracking_node(artifact: TrackingArtifact) -> EditableNode<TrackingArtifact> {
    let artifact_type = ValueTypeId::from_str("superi.value.tracking_artifact").unwrap();
    let definition = EffectNodeDefinition::new(
        NodeSchemaId::new(
            NodeTypeId::from_str("superi.effects.tracking").unwrap(),
            SemanticVersion::new(1, 0, 0),
        ),
        EffectMetadata::new(
            "Tracking",
            "Stores editable selections, corrections, observations, and solved motion.",
            "Tracking",
        )
        .unwrap(),
        [],
        [],
        [EffectParameterDefinition::new(
            ParameterSchema::new(
                ParameterName::new("artifact").unwrap(),
                artifact_type.clone(),
                true,
            ),
            "Artifact",
            "Editable exact-frame tracking state.",
            ParameterControl::Automatic,
            TypedParameterValue::new(artifact_type, artifact),
        )
        .unwrap()],
        NodeBehavior::new(
            TimeBehavior::CurrentFrame,
            RoiBehavior::InputBounds,
            ColorRequirements::Exact(ColorSpace::ACESCG),
            Determinism::Deterministic,
            CachePolicy::PerRegion,
        ),
        CapabilitySet::default(),
    )
    .unwrap();
    assert!(definition
        .parameter(&ParameterName::new("artifact").unwrap())
        .unwrap()
        .schema()
        .is_animatable());
    definition
        .instantiate(
            EffectInstanceBindings::new(
                [],
                [],
                [EffectParameterBinding::new(
                    ParameterId::from_raw(901),
                    ParameterName::new("artifact").unwrap(),
                )],
            ),
            [],
        )
        .unwrap()
}

#[test]
fn complete_tracking_state_is_editable_and_reusable_through_graph_reload() {
    let artifact = all_kind_artifact();
    let graph_value = GraphValue::Domain(artifact.clone());
    assert_eq!(graph_value.as_domain(), Some(&artifact));

    for (graph_id, node_id) in [(701, 71), (702, 72)] {
        let mut graph = EditableGraph::new(GraphId::from_raw(graph_id));
        graph
            .apply(GraphTransaction::with_mutations(
                0,
                [GraphMutation::Add {
                    node_id: NodeId::from_raw(node_id),
                    node: tracking_node(artifact.clone()),
                    position: 0,
                }],
            ))
            .unwrap();
        let bytes = serialize_graph(&graph.snapshot()).unwrap();
        let loaded = deserialize_graph::<TrackingArtifact>(&bytes).unwrap();
        let snapshot = loaded.graph().snapshot();
        let reloaded = snapshot
            .node(NodeId::from_raw(node_id))
            .unwrap()
            .parameter(ParameterId::from_raw(901))
            .unwrap()
            .value()
            .payload();
        assert_eq!(reloaded, &artifact);
        assert_eq!(reloaded.tracks().len(), 4);
        assert_eq!(serialize_graph(&loaded.graph().snapshot()).unwrap(), bytes);
    }

    let refined = artifact
        .with_replaced_track(
            TrackingTrack::new(
                TrackId::from_raw(1),
                0,
                TrackingSelection::point(TrackedFeature::new(
                    11,
                    TrackingPoint::new(32.0, 30.0).unwrap(),
                )),
            )
            .unwrap(),
        )
        .unwrap();
    assert_eq!(refined.revision(), artifact.revision() + 1);
    assert!(refined
        .track(TrackId::from_raw(1))
        .unwrap()
        .corrections()
        .is_empty());
    assert_eq!(
        refined
            .without_track(TrackId::from_raw(4))
            .unwrap()
            .tracks()
            .len(),
        3
    );
}

#[test]
fn external_solver_results_are_checked_against_the_exact_request() {
    let id = TrackId::from_raw(90);
    let point = TrackingPoint::new(20.0, 20.0).unwrap();
    let artifact = TrackingArtifact::new(
        Timebase::integer(24).unwrap(),
        [TrackingTrack::new(
            id,
            0,
            TrackingSelection::point(TrackedFeature::new(91, point)),
        )
        .unwrap()],
    )
    .unwrap();
    let request = artifact.solve_request(id, 1).unwrap();
    let target = TrackingPoint::new(21.0, 20.5).unwrap();
    let sample = TrackingSample::new(
        1,
        TrackingModel::Point { position: target },
        [TrackingObservation::new(FeatureId::from_raw(91), target, 0.9).unwrap()],
    )
    .unwrap();
    let result = TrackingResult::new(&request, sample).unwrap();
    assert!(artifact.apply_solver_result(result).is_ok());

    let wrong_frame = TrackingSample::new(
        2,
        TrackingModel::Point { position: target },
        [TrackingObservation::new(FeatureId::from_raw(91), target, 0.9).unwrap()],
    )
    .unwrap();
    assert!(TrackingResult::new(&request, wrong_frame).is_err());

    let wrong_feature = TrackingSample::new(
        1,
        TrackingModel::Point { position: target },
        [TrackingObservation::new(FeatureId::from_raw(92), target, 0.9).unwrap()],
    )
    .unwrap();
    assert!(TrackingResult::new(&request, wrong_feature).is_err());
}

#[test]
fn persisted_tracking_geometry_round_trips_the_shared_core_contract() {
    let core_point = Point2::new(12.5, -4.25).unwrap();
    assert_eq!(
        TrackingPoint::from_core(core_point).into_core().unwrap(),
        core_point
    );

    let core_matrix =
        Matrix3::from_rows([[1.0, 0.1, 2.0], [-0.2, 0.9, 3.0], [0.0, 0.0, 1.0]]).unwrap();
    assert_eq!(
        TrackingMatrix3::from_core(core_matrix).into_core().unwrap(),
        core_matrix
    );

    let core_region = Rect::new(
        Point2::new(2.0, 3.0).unwrap(),
        Point2::new(20.0, 30.0).unwrap(),
    )
    .unwrap();
    assert_eq!(
        TrackingRect::from_core(core_region).into_core().unwrap(),
        core_region
    );
}
