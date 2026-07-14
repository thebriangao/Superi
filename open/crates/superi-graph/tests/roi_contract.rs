use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;

use superi_core::error::ErrorCategory;
use superi_core::geometry::PixelBounds;
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_graph::dag::{GraphEdge, GraphEndpoint};
use superi_graph::ids::{EdgeId, GraphId, NodeId, PortId};
use superi_graph::invalidation::{
    propagate_dependency_invalidation, DirtyRegion, DirtyRegionSet, InvalidationSeed,
};
use superi_graph::mutate::{
    EditableGraph, EditableNode, GraphMutation, GraphSnapshot, GraphTransaction, InstancePort,
};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchema, NodeSchemaId,
    NodeTypeId, PortCardinality, PortName, PortSchema, RoiBehavior, TimeBehavior, ValueTypeId,
};
use superi_graph::roi::{
    propagate_roi, propagate_roi_with, CustomRoiRequest, RoiDomains, RoiRequest,
};

fn bounds(min_x: i32, min_y: i32, max_x: i32, max_y: i32) -> PixelBounds {
    PixelBounds::new(min_x, min_y, max_x, max_y).expect("test bounds must be valid")
}

fn regions(values: impl IntoIterator<Item = PixelBounds>) -> DirtyRegionSet {
    let mut result = DirtyRegionSet::empty();
    for value in values {
        result.insert(DirtyRegion::Bounds(value));
    }
    result
}

fn endpoint(node: u128, port: u128) -> GraphEndpoint {
    GraphEndpoint::new(NodeId::from_raw(node), PortId::from_raw(port))
}

fn edge(
    id: u128,
    source_node: u128,
    source_port: u128,
    destination_node: u128,
    destination_port: u128,
) -> GraphEdge {
    GraphEdge::new(
        EdgeId::from_raw(id),
        endpoint(source_node, source_port),
        endpoint(destination_node, destination_port),
    )
}

fn editable_node(
    node_type: &str,
    roi: RoiBehavior,
    inputs: &[(u128, &str)],
    outputs: &[(u128, &str)],
) -> EditableNode<()> {
    let image_type = ValueTypeId::from_str("superi.value.image").expect("valid value type");
    let input_names = inputs
        .iter()
        .map(|(_, name)| PortName::new(*name).expect("valid input name"))
        .collect::<Vec<_>>();
    let output_names = outputs
        .iter()
        .map(|(_, name)| PortName::new(*name).expect("valid output name"))
        .collect::<Vec<_>>();
    let schema = Arc::new(
        NodeSchema::new(
            NodeSchemaId::new(
                NodeTypeId::from_str(node_type).expect("valid node type"),
                SemanticVersion::new(1, 0, 0),
            ),
            input_names.iter().map(|name| {
                PortSchema::new(name.clone(), image_type.clone(), PortCardinality::Single)
            }),
            output_names.iter().map(|name| {
                PortSchema::new(name.clone(), image_type.clone(), PortCardinality::Single)
            }),
            [],
            NodeBehavior::new(
                TimeBehavior::CurrentFrame,
                roi,
                ColorRequirements::NotApplicable,
                Determinism::Deterministic,
                CachePolicy::PerRegion,
            ),
            CapabilitySet::default(),
        )
        .expect("valid test schema"),
    );

    EditableNode::new(
        schema,
        inputs
            .iter()
            .zip(input_names)
            .map(|((id, _), name)| InstancePort::new(PortId::from_raw(*id), name)),
        outputs
            .iter()
            .zip(output_names)
            .map(|((id, _), name)| InstancePort::new(PortId::from_raw(*id), name)),
        [],
    )
    .expect("valid editable node")
}

struct TestGraph {
    snapshot: GraphSnapshot<()>,
    domains: RoiDomains,
    source_a_output: GraphEndpoint,
    source_b_output: GraphEndpoint,
    process_input_a: GraphEndpoint,
    process_input_b: GraphEndpoint,
    process_output: GraphEndpoint,
    sink_input: GraphEndpoint,
    sink_output: GraphEndpoint,
    unrelated_output: GraphEndpoint,
}

fn test_graph(process_roi: RoiBehavior, reverse_insertion: bool) -> TestGraph {
    let source_a_output = endpoint(1, 101);
    let source_b_output = endpoint(2, 201);
    let process_input_a = endpoint(3, 301);
    let process_input_b = endpoint(3, 302);
    let process_output = endpoint(3, 303);
    let sink_input = endpoint(4, 401);
    let sink_output = endpoint(4, 402);
    let unrelated_output = endpoint(9, 901);

    let nodes = [
        (
            NodeId::from_raw(1),
            editable_node(
                "superi.source.a",
                RoiBehavior::InputBounds,
                &[],
                &[(101, "image")],
            ),
        ),
        (
            NodeId::from_raw(2),
            editable_node(
                "superi.source.b",
                RoiBehavior::InputBounds,
                &[],
                &[(201, "image")],
            ),
        ),
        (
            NodeId::from_raw(3),
            editable_node(
                "superi.process",
                process_roi,
                &[(301, "primary"), (302, "secondary")],
                &[(303, "image")],
            ),
        ),
        (
            NodeId::from_raw(4),
            editable_node(
                "superi.sink",
                RoiBehavior::InputBounds,
                &[(401, "image")],
                &[(402, "image")],
            ),
        ),
        (
            NodeId::from_raw(9),
            editable_node(
                "superi.unrelated",
                RoiBehavior::InputBounds,
                &[],
                &[(901, "image")],
            ),
        ),
    ];
    let connections = [
        edge(11, 1, 101, 3, 301),
        edge(12, 2, 201, 3, 302),
        edge(13, 3, 303, 4, 401),
    ];

    let mut mutations = Vec::new();
    if reverse_insertion {
        for (node_id, node) in nodes.into_iter().rev() {
            mutations.push(GraphMutation::Add {
                node_id,
                node,
                position: 0,
            });
        }
        for graph_edge in connections.into_iter().rev() {
            mutations.push(GraphMutation::Connect { edge: graph_edge });
        }
    } else {
        for (position, (node_id, node)) in nodes.into_iter().enumerate() {
            mutations.push(GraphMutation::Add {
                node_id,
                node,
                position,
            });
        }
        for graph_edge in connections {
            mutations.push(GraphMutation::Connect { edge: graph_edge });
        }
    }

    let mut graph = EditableGraph::new(GraphId::from_raw(800));
    let snapshot = graph
        .apply(GraphTransaction::with_mutations(0, mutations))
        .expect("test graph must publish atomically");
    let domains = RoiDomains::new([
        (source_a_output, bounds(0, 0, 50, 50)),
        (source_b_output, bounds(10, 10, 80, 80)),
        (process_output, bounds(0, 0, 100, 100)),
        (sink_output, bounds(0, 0, 100, 100)),
        (unrelated_output, bounds(0, 0, 100, 100)),
    ])
    .expect("test domains must be unique");

    TestGraph {
        snapshot,
        domains,
        source_a_output,
        source_b_output,
        process_input_a,
        process_input_b,
        process_output,
        sink_input,
        sink_output,
        unrelated_output,
    }
}

#[test]
fn pass_through_propagation_prunes_unrelated_work_and_retains_dependency_order() {
    let graph = test_graph(RoiBehavior::InputBounds, false);
    let requested = bounds(20, 20, 40, 40);
    let plan = propagate_roi(
        &graph.snapshot,
        &graph.domains,
        [RoiRequest::from_bounds(graph.sink_output, requested)],
    )
    .expect("valid pass-through ROI must propagate");

    assert_eq!(plan.graph_id(), GraphId::from_raw(800));
    assert_eq!(plan.graph_revision(), 1);
    assert_eq!(
        plan.evaluation_order(),
        &[
            NodeId::from_raw(1),
            NodeId::from_raw(2),
            NodeId::from_raw(3),
            NodeId::from_raw(4),
        ]
    );
    for output in [
        graph.source_a_output,
        graph.source_b_output,
        graph.process_output,
        graph.sink_output,
    ] {
        assert_eq!(
            plan.output_regions(output),
            Some(&regions([requested])),
            "every connected output must retain the requested extent"
        );
    }
    assert_eq!(
        plan.input_regions(graph.process_input_a),
        Some(&regions([requested]))
    );
    assert_eq!(
        plan.input_regions(graph.process_input_b),
        Some(&regions([requested]))
    );
    assert_eq!(
        plan.input_regions(graph.sink_input),
        Some(&regions([requested]))
    );
    assert!(plan.output_regions(graph.unrelated_output).is_none());
}

#[test]
fn repeated_requests_and_fan_out_preserve_exact_regions_without_bounding_clean_gaps() {
    let graph = test_graph(RoiBehavior::InputBounds, false);
    let requested = regions([bounds(20, 20, 24, 22), bounds(20, 20, 22, 24)]);
    let plan = propagate_roi(
        &graph.snapshot,
        &graph.domains,
        [
            RoiRequest::new(graph.sink_output, requested.clone()),
            RoiRequest::from_bounds(graph.sink_output, bounds(21, 21, 21, 23)),
        ],
    )
    .expect("exact requests must merge");

    assert_eq!(plan.output_regions(graph.source_a_output), Some(&requested));
    assert_eq!(plan.output_regions(graph.source_b_output), Some(&requested));
    assert_eq!(
        plan.output_regions(graph.source_a_output)
            .expect("source must be required")
            .regions(),
        &[bounds(20, 20, 22, 24), bounds(22, 20, 24, 22)]
    );
}

#[test]
fn full_frame_and_expanded_behaviors_use_each_upstream_region_of_definition() {
    let full_frame_graph = test_graph(RoiBehavior::FullFrame, false);
    let full_frame = propagate_roi(
        &full_frame_graph.snapshot,
        &full_frame_graph.domains,
        [RoiRequest::from_bounds(
            full_frame_graph.sink_output,
            bounds(20, 20, 24, 24),
        )],
    )
    .expect("full-frame mapping must use exact upstream domains");
    assert_eq!(
        full_frame.output_regions(full_frame_graph.source_a_output),
        Some(&regions([bounds(0, 0, 50, 50)]))
    );
    assert_eq!(
        full_frame.output_regions(full_frame_graph.source_b_output),
        Some(&regions([bounds(10, 10, 80, 80)]))
    );

    let expanded_graph = test_graph(RoiBehavior::Expanded { pixels: 3 }, false);
    let expanded = propagate_roi(
        &expanded_graph.snapshot,
        &expanded_graph.domains,
        [RoiRequest::from_bounds(
            expanded_graph.sink_output,
            bounds(20, 20, 40, 40),
        )],
    )
    .expect("expanded mapping must be checked and clipped");
    assert_eq!(
        expanded.output_regions(expanded_graph.source_a_output),
        Some(&regions([bounds(17, 17, 43, 43)]))
    );
    assert_eq!(
        expanded.output_regions(expanded_graph.source_b_output),
        Some(&regions([bounds(17, 17, 43, 43)]))
    );
}

#[test]
fn checked_expansion_rejects_coordinate_overflow() {
    let mut graph = test_graph(RoiBehavior::Expanded { pixels: 4 }, false);
    graph.domains = RoiDomains::new([
        (
            graph.source_a_output,
            bounds(i32::MIN, i32::MIN, i32::MIN + 20, i32::MIN + 20),
        ),
        (
            graph.source_b_output,
            bounds(i32::MIN, i32::MIN, i32::MIN + 20, i32::MIN + 20),
        ),
        (
            graph.process_output,
            bounds(i32::MIN, i32::MIN, i32::MIN + 20, i32::MIN + 20),
        ),
        (
            graph.sink_output,
            bounds(i32::MIN, i32::MIN, i32::MIN + 20, i32::MIN + 20),
        ),
    ])
    .expect("overflow test domains must be unique");

    let error = propagate_roi(
        &graph.snapshot,
        &graph.domains,
        [RoiRequest::from_bounds(
            graph.sink_output,
            bounds(i32::MIN, i32::MIN, i32::MIN + 8, i32::MIN + 8),
        )],
    )
    .expect_err("expansion beyond signed coordinates must fail");

    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.contexts()[0].component(), "superi-graph.roi");
    assert_eq!(error.contexts()[0].operation(), "expand_regions");
}

#[test]
fn custom_mapping_receives_exact_outputs_and_returns_per_input_work() {
    let graph = test_graph(RoiBehavior::Custom, false);
    let output_request = bounds(20, 20, 40, 40);
    let mut visited = Vec::new();
    let plan = propagate_roi_with(
        &graph.snapshot,
        &graph.domains,
        [RoiRequest::from_bounds(graph.sink_output, output_request)],
        |request: CustomRoiRequest<'_, ()>| {
            visited.push(request.node_id());
            assert_eq!(
                request.node().schema().behavior().roi(),
                RoiBehavior::Custom
            );
            assert_eq!(
                request.requested_outputs().get(&PortId::from_raw(303)),
                Some(&regions([output_request]))
            );
            Ok(BTreeMap::from([
                (PortId::from_raw(301), regions([bounds(18, 18, 42, 42)])),
                (PortId::from_raw(302), regions([bounds(24, 24, 36, 36)])),
            ]))
        },
    )
    .expect("custom mapping must preserve exact input identity");

    assert_eq!(visited, vec![NodeId::from_raw(3)]);
    assert_eq!(
        plan.output_regions(graph.source_a_output),
        Some(&regions([bounds(18, 18, 42, 42)]))
    );
    assert_eq!(
        plan.output_regions(graph.source_b_output),
        Some(&regions([bounds(24, 24, 36, 36)]))
    );
}

#[test]
fn custom_and_request_contract_failures_are_actionable_and_atomic() {
    let graph = test_graph(RoiBehavior::Custom, false);
    let request = RoiRequest::from_bounds(graph.sink_output, bounds(20, 20, 40, 40));
    let missing_mapper = propagate_roi(&graph.snapshot, &graph.domains, [request.clone()])
        .expect_err("custom behavior requires a mapper");
    assert_eq!(
        missing_mapper.contexts()[0].operation(),
        "map_custom_inputs"
    );

    let invalid_port = propagate_roi_with(
        &graph.snapshot,
        &graph.domains,
        [request],
        |_request: CustomRoiRequest<'_, ()>| {
            Ok(BTreeMap::from([(
                PortId::from_raw(303),
                regions([bounds(1, 1, 2, 2)]),
            )]))
        },
    )
    .expect_err("custom mapping cannot return an output port as input work");
    assert_eq!(invalid_port.category(), ErrorCategory::Internal);
    assert_eq!(
        invalid_port.contexts()[0].operation(),
        "validate_custom_inputs"
    );

    let pass_through = test_graph(RoiBehavior::InputBounds, false);
    let input_request = propagate_roi(
        &pass_through.snapshot,
        &pass_through.domains,
        [RoiRequest::from_bounds(
            graph.process_input_a,
            bounds(1, 1, 2, 2),
        )],
    )
    .expect_err("evaluation requests must identify an output port");
    assert_eq!(input_request.category(), ErrorCategory::InvalidInput);
    assert_eq!(input_request.contexts()[0].operation(), "validate_request");

    let empty = propagate_roi(
        &pass_through.snapshot,
        &pass_through.domains,
        [RoiRequest::from_bounds(
            pass_through.sink_output,
            bounds(200, 200, 210, 210),
        )],
    )
    .expect("a request outside the output domain is valid empty work");
    assert!(empty.is_empty());
    assert!(empty.outputs().is_empty());

    let missing_source_domain = RoiDomains::new([
        (pass_through.source_a_output, bounds(0, 0, 50, 50)),
        (pass_through.process_output, bounds(0, 0, 100, 100)),
        (pass_through.sink_output, bounds(0, 0, 100, 100)),
    ])
    .expect("remaining domains are unique");
    let missing_domain = propagate_roi(
        &pass_through.snapshot,
        &missing_source_domain,
        [RoiRequest::from_bounds(
            pass_through.sink_output,
            bounds(20, 20, 40, 40),
        )],
    )
    .expect_err("every reached output requires a current domain");
    assert_eq!(missing_domain.category(), ErrorCategory::NotFound);
    assert_eq!(missing_domain.contexts()[0].operation(), "resolve_domain");

    let duplicate_domain = RoiDomains::new([
        (pass_through.sink_output, bounds(0, 0, 100, 100)),
        (pass_through.sink_output, bounds(0, 0, 100, 100)),
    ])
    .expect_err("duplicate domain declarations must not overwrite");
    assert_eq!(duplicate_domain.category(), ErrorCategory::InvalidInput);
    assert_eq!(duplicate_domain.contexts()[0].operation(), "create_domains");
}

#[test]
fn invalidation_intersection_preserves_clean_gaps_and_excludes_clean_outputs() {
    let graph = test_graph(RoiBehavior::InputBounds, false);
    let requested = bounds(0, 0, 10, 10);
    let roi = propagate_roi(
        &graph.snapshot,
        &graph.domains,
        [RoiRequest::from_bounds(graph.sink_output, requested)],
    )
    .expect("ROI plan must succeed");
    let dirty = regions([bounds(0, 0, 4, 2), bounds(0, 0, 2, 4)]);
    let invalidation = propagate_dependency_invalidation(
        graph.snapshot.dag(),
        [InvalidationSeed::new(
            graph.source_a_output.node_id(),
            dirty.clone(),
        )],
    )
    .expect("invalidation plan must succeed");
    let work = roi.invalidated_output_work(&invalidation);

    assert_eq!(work.get(&graph.source_a_output), Some(&dirty));
    assert_eq!(work.get(&graph.process_output), Some(&dirty));
    assert_eq!(work.get(&graph.sink_output), Some(&dirty));
    assert!(!work.contains_key(&graph.source_b_output));
    assert!(!work.contains_key(&graph.unrelated_output));
}

#[test]
fn equivalent_snapshots_produce_identical_editor_script_and_headless_plans() {
    let editor_graph = test_graph(RoiBehavior::InputBounds, false);
    let headless_graph = test_graph(RoiBehavior::InputBounds, true);
    let request = RoiRequest::from_bounds(editor_graph.sink_output, bounds(20, 20, 40, 40));

    let editor = propagate_roi(
        &editor_graph.snapshot,
        &editor_graph.domains,
        [request.clone()],
    )
    .expect("editor ROI must succeed");
    let script_snapshot = editor_graph.snapshot.clone();
    let script = propagate_roi(&script_snapshot, &editor_graph.domains, [request.clone()])
        .expect("script ROI must succeed");
    let headless = propagate_roi(&headless_graph.snapshot, &headless_graph.domains, [request])
        .expect("headless ROI must succeed");

    assert_eq!(editor, script);
    assert_eq!(editor, headless);
}
