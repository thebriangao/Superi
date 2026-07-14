use std::str::FromStr;
use std::sync::Arc;

use superi_core::geometry::PixelBounds;
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_graph::dag::{DirectedAcyclicGraph, GraphEdge, GraphEndpoint};
use superi_graph::ids::{EdgeId, GraphId, NodeId, PortId};
use superi_graph::invalidation::{
    propagate_dependency_invalidation, propagate_invalidation_with, DirtyRegion, DirtyRegionSet,
    InvalidationSeed,
};
use superi_graph::mutate::{
    EditableGraph, EditableNode, GraphMutation, GraphTransaction, InstancePort,
};
use superi_graph::node::{
    CachePolicy, ColorRequirements, Determinism, NodeBehavior, NodeSchema, NodeSchemaId,
    NodeTypeId, PortCardinality, PortName, PortSchema, RoiBehavior, TimeBehavior, ValueTypeId,
};

fn bounds(min_x: i32, min_y: i32, max_x: i32, max_y: i32) -> PixelBounds {
    PixelBounds::new(min_x, min_y, max_x, max_y).expect("test bounds must be valid")
}

#[test]
fn dirty_region_sets_preserve_exact_work_without_clean_gaps() {
    let mut dirty = DirtyRegionSet::empty();
    dirty.insert(DirtyRegion::Bounds(bounds(0, 0, 4, 2)));
    dirty.insert(DirtyRegion::Bounds(bounds(0, 0, 2, 4)));
    dirty.insert(DirtyRegion::Bounds(bounds(1, 1, 1, 3)));

    assert_eq!(dirty.regions(), &[bounds(0, 0, 2, 4), bounds(2, 0, 4, 2)]);

    let clipped = dirty.clip_to(bounds(1, 1, 3, 3));
    assert_eq!(clipped.regions(), &[bounds(1, 1, 2, 3), bounds(2, 1, 3, 2)]);
    assert!(clipped.clip_to(bounds(2, 2, 3, 3)).is_empty());
}

#[test]
fn full_frame_subsumes_finite_regions_and_clips_to_requested_work() {
    let mut dirty = DirtyRegionSet::from_region(DirtyRegion::Bounds(bounds(-4, -2, 8, 6)));
    dirty.insert(DirtyRegion::FullFrame);
    dirty.insert(DirtyRegion::Bounds(bounds(20, 20, 30, 30)));

    assert!(dirty.is_full_frame());
    assert!(dirty.regions().is_empty());

    let requested = bounds(100, 40, 120, 60);
    assert_eq!(
        dirty.clip_to(requested),
        DirtyRegionSet::from_region(DirtyRegion::Bounds(requested))
    );
    assert!(dirty.clip_to(bounds(3, 3, 3, 9)).is_empty());
}

fn edge(id: u128, source: u128, destination: u128) -> GraphEdge {
    GraphEdge::new(
        EdgeId::from_raw(id),
        GraphEndpoint::new(NodeId::from_raw(source), PortId::from_raw(id * 2)),
        GraphEndpoint::new(NodeId::from_raw(destination), PortId::from_raw(id * 2 + 1)),
    )
}

fn diamond_graph(node_order: &[u128], edge_order: &[u128]) -> DirectedAcyclicGraph<&'static str> {
    let mut graph = DirectedAcyclicGraph::new(GraphId::from_raw(700));
    for node_id in node_order {
        graph
            .insert_node(NodeId::from_raw(*node_id), "node")
            .expect("test nodes must be unique");
    }
    let edges = [
        edge(10, 1, 2),
        edge(11, 1, 3),
        edge(12, 2, 4),
        edge(13, 3, 4),
    ];
    for edge_id in edge_order {
        let graph_edge = edges
            .iter()
            .find(|candidate| candidate.id().raw() == *edge_id)
            .copied()
            .expect("test edge ID must exist");
        graph
            .insert_edge(graph_edge)
            .expect("test edge must form a DAG");
    }
    graph
}

fn editable_node(
    node_type: &str,
    input: Option<(u128, &str)>,
    output: Option<(u128, &str)>,
) -> EditableNode<()> {
    let image_type = ValueTypeId::from_str("superi.value.image").expect("valid value type");
    let input_name = input.map(|(_, name)| PortName::new(name).expect("valid input name"));
    let output_name = output.map(|(_, name)| PortName::new(name).expect("valid output name"));
    let schema = Arc::new(
        NodeSchema::new(
            NodeSchemaId::new(
                NodeTypeId::from_str(node_type).expect("valid node type"),
                SemanticVersion::new(1, 0, 0),
            ),
            input_name.iter().map(|name| {
                PortSchema::new(name.clone(), image_type.clone(), PortCardinality::Single)
            }),
            output_name.iter().map(|name| {
                PortSchema::new(name.clone(), image_type.clone(), PortCardinality::Single)
            }),
            [],
            NodeBehavior::new(
                TimeBehavior::CurrentFrame,
                RoiBehavior::InputBounds,
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
        input
            .into_iter()
            .zip(input_name)
            .map(|((id, _), name)| InstancePort::new(PortId::from_raw(id), name)),
        output
            .into_iter()
            .zip(output_name)
            .map(|((id, _), name)| InstancePort::new(PortId::from_raw(id), name)),
        [],
    )
    .expect("valid editable test node")
}

#[test]
fn dependency_invalidation_is_topological_exact_and_excludes_clean_nodes() {
    let graph = diamond_graph(&[99, 4, 3, 2, 1], &[13, 12, 11, 10]);
    let dirty = DirtyRegionSet::from_region(DirtyRegion::Bounds(bounds(0, 0, 16, 16)));
    let plan = propagate_dependency_invalidation(
        &graph,
        [InvalidationSeed::new(NodeId::from_raw(1), dirty.clone())],
    )
    .expect("valid graph invalidation must succeed");

    assert_eq!(
        plan.nodes()
            .iter()
            .map(|node| node.node_id().raw())
            .collect::<Vec<_>>(),
        vec![1, 2, 3, 4]
    );
    assert!(plan.dirty_regions(NodeId::from_raw(99)).is_none());
    for node_id in [1, 2, 3, 4] {
        assert_eq!(plan.dirty_regions(NodeId::from_raw(node_id)), Some(&dirty));
    }
    assert_eq!(
        plan.requested_work(NodeId::from_raw(4), bounds(8, 8, 24, 24)),
        DirtyRegionSet::from_region(DirtyRegion::Bounds(bounds(8, 8, 16, 16)))
    );
}

#[test]
fn converging_seeds_merge_without_over_invalidating() {
    let graph = diamond_graph(&[1, 2, 3, 4, 99], &[10, 11, 12, 13]);
    let plan = propagate_dependency_invalidation(
        &graph,
        [
            InvalidationSeed::from_region(
                NodeId::from_raw(2),
                DirtyRegion::Bounds(bounds(0, 0, 4, 2)),
            ),
            InvalidationSeed::from_region(
                NodeId::from_raw(3),
                DirtyRegion::Bounds(bounds(0, 0, 2, 4)),
            ),
        ],
    )
    .expect("converging invalidation must succeed");

    assert_eq!(
        plan.dirty_regions(NodeId::from_raw(4))
            .expect("shared descendant must be dirty")
            .regions(),
        &[bounds(0, 0, 2, 4), bounds(2, 0, 4, 2)]
    );
}

#[test]
fn edge_mapping_can_stop_or_transform_exact_dependency_branches() {
    let graph = diamond_graph(&[1, 2, 3, 4, 99], &[10, 11, 12, 13]);
    let mut visited_edges = Vec::new();
    let plan = propagate_invalidation_with(
        &graph,
        [InvalidationSeed::from_region(
            NodeId::from_raw(1),
            DirtyRegion::Bounds(bounds(10, 10, 20, 20)),
        )],
        |_, graph_edge, dirty| {
            visited_edges.push(graph_edge.id().raw());
            if graph_edge.id() == EdgeId::from_raw(10) {
                return Ok(DirtyRegionSet::empty());
            }
            if graph_edge.id() == EdgeId::from_raw(11) {
                return Ok(DirtyRegionSet::from_region(DirtyRegion::Bounds(bounds(
                    12, 12, 18, 18,
                ))));
            }
            Ok(dirty.clone())
        },
    )
    .expect("edge-aware invalidation must succeed");

    assert_eq!(visited_edges, vec![10, 11, 13]);
    assert!(plan.dirty_regions(NodeId::from_raw(2)).is_none());
    assert_eq!(
        plan.dirty_regions(NodeId::from_raw(3))
            .expect("mapped branch must be dirty")
            .regions(),
        &[bounds(12, 12, 18, 18)]
    );
    assert_eq!(
        plan.dirty_regions(NodeId::from_raw(4))
            .expect("mapped descendant must be dirty")
            .regions(),
        &[bounds(12, 12, 18, 18)]
    );
}

#[test]
fn unknown_seed_nodes_fail_with_actionable_context() {
    let graph = diamond_graph(&[1, 2, 3, 4, 99], &[10, 11, 12, 13]);
    let error = propagate_dependency_invalidation(
        &graph,
        [InvalidationSeed::from_region(
            NodeId::from_raw(404),
            DirtyRegion::FullFrame,
        )],
    )
    .expect_err("unknown seed must fail");

    assert_eq!(
        error.category(),
        superi_core::error::ErrorCategory::NotFound
    );
    assert_eq!(error.contexts().len(), 1);
    assert_eq!(error.contexts()[0].operation(), "propagate_invalidation");
    assert_eq!(
        error.contexts()[0].field("graph_id"),
        Some("graph:000000000000000000000000000002bc")
    );
    assert_eq!(
        error.contexts()[0].field("node_id"),
        Some("node:00000000000000000000000000000194")
    );
}

#[test]
fn empty_work_stops_before_edge_mapping_and_mapper_errors_gain_edge_context() {
    let graph = diamond_graph(&[1, 2, 3, 4, 99], &[10, 11, 12, 13]);
    let empty = propagate_invalidation_with(
        &graph,
        [InvalidationSeed::from_region(
            NodeId::from_raw(1),
            DirtyRegion::Bounds(bounds(2, 2, 2, 8)),
        )],
        |_, _, _| -> superi_core::error::Result<DirtyRegionSet> {
            panic!("empty dirty work must not reach the edge mapper")
        },
    )
    .expect("empty work must produce an empty plan");
    assert!(empty.is_empty());

    let error = propagate_invalidation_with(
        &graph,
        [InvalidationSeed::from_region(
            NodeId::from_raw(1),
            DirtyRegion::FullFrame,
        )],
        |_, _, _| {
            Err(superi_core::error::Error::new(
                superi_core::error::ErrorCategory::Unavailable,
                superi_core::error::Recoverability::Retryable,
                "test mapper unavailable",
            ))
        },
    )
    .expect_err("mapper failure must abort the derived plan");

    assert_eq!(
        error.category(),
        superi_core::error::ErrorCategory::Unavailable
    );
    assert_eq!(error.contexts().len(), 1);
    assert_eq!(error.contexts()[0].operation(), "map_dirty_edge");
    assert_eq!(
        error.contexts()[0].field("edge_id"),
        Some("edge:0000000000000000000000000000000a")
    );
    assert_eq!(
        error.contexts()[0].field("source_node_id"),
        Some("node:00000000000000000000000000000001")
    );
    assert_eq!(
        error.contexts()[0].field("destination_node_id"),
        Some("node:00000000000000000000000000000002")
    );
}

#[test]
fn editor_script_and_headless_paths_observe_identical_plans() {
    let editor_graph = diamond_graph(&[1, 2, 3, 4, 99], &[10, 11, 12, 13]);
    let headless_graph = diamond_graph(&[99, 4, 3, 2, 1], &[13, 12, 11, 10]);
    let seeds = [InvalidationSeed::from_region(
        NodeId::from_raw(1),
        DirtyRegion::Bounds(bounds(-8, -8, 8, 8)),
    )];

    let editor = propagate_dependency_invalidation(&editor_graph, seeds.clone())
        .expect("editor plan must succeed");
    let script = propagate_dependency_invalidation(&editor_graph, seeds.clone())
        .expect("script plan must succeed");
    let headless = propagate_dependency_invalidation(&headless_graph, seeds)
        .expect("headless plan must succeed");

    assert_eq!(editor, script);
    assert_eq!(editor, headless);
}

#[test]
fn published_editable_snapshot_is_the_single_invalidation_source_for_all_readers() {
    let mut editable = EditableGraph::new(GraphId::from_raw(701));
    let snapshot = editable
        .apply(GraphTransaction::with_mutations(
            0,
            [
                GraphMutation::Add {
                    node_id: NodeId::from_raw(1),
                    node: editable_node("superi.source.image", None, Some((20, "image"))),
                    position: 0,
                },
                GraphMutation::Add {
                    node_id: NodeId::from_raw(2),
                    node: editable_node("superi.output.image", Some((21, "image")), None),
                    position: 1,
                },
                GraphMutation::Connect {
                    edge: edge(10, 1, 2),
                },
            ],
        ))
        .expect("editable graph transaction must publish");
    let seeds = [InvalidationSeed::from_region(
        NodeId::from_raw(1),
        DirtyRegion::Bounds(bounds(4, 4, 12, 12)),
    )];

    let editor = propagate_dependency_invalidation(snapshot.dag(), seeds.clone())
        .expect("editor invalidation must succeed");
    let script_snapshot = snapshot.clone();
    let script = propagate_dependency_invalidation(script_snapshot.dag(), seeds.clone())
        .expect("script invalidation must succeed");
    let headless_snapshot = snapshot.clone();
    let headless = propagate_dependency_invalidation(headless_snapshot.dag(), seeds)
        .expect("headless invalidation must succeed");

    assert_eq!(snapshot.revision(), 1);
    assert_eq!(editor, script);
    assert_eq!(editor, headless);
    assert_eq!(
        editor
            .nodes()
            .iter()
            .map(|node| node.node_id().raw())
            .collect::<Vec<_>>(),
        vec![1, 2]
    );
}
