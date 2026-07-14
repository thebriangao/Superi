use std::any::TypeId;
use std::str::FromStr;

use superi_core::ids::{
    EdgeId as CoreEdgeId, GraphId as CoreGraphId, NodeId as CoreNodeId,
    ParameterId as CoreParameterId, PortId as CorePortId, ResourceId as CoreResourceId, TypedId,
};
use superi_graph::ids::{EdgeId, GraphId, NodeId, ParameterId, PortId, ResourceId};

fn assert_same_type<T: 'static, U: 'static>() {
    assert_eq!(TypeId::of::<T>(), TypeId::of::<U>());
}

fn assert_canonical<T>(prefix: &str)
where
    T: TypedId + FromStr + PartialEq + std::fmt::Debug + std::fmt::Display,
    <T as FromStr>::Err: std::fmt::Debug,
{
    let id = T::from_raw(0x2a);
    let text = format!("{prefix}:0000000000000000000000000000002a");
    assert_eq!(id.to_string(), text);
    assert_eq!(text.parse::<T>().unwrap(), id);
}

#[test]
fn graph_exports_the_official_core_identifier_types() {
    assert_same_type::<GraphId, CoreGraphId>();
    assert_same_type::<NodeId, CoreNodeId>();
    assert_same_type::<PortId, CorePortId>();
    assert_same_type::<EdgeId, CoreEdgeId>();
    assert_same_type::<ParameterId, CoreParameterId>();
    assert_same_type::<ResourceId, CoreResourceId>();
}

#[test]
fn graph_identifier_domains_are_distinct_and_canonical() {
    let types = [
        TypeId::of::<GraphId>(),
        TypeId::of::<NodeId>(),
        TypeId::of::<PortId>(),
        TypeId::of::<EdgeId>(),
        TypeId::of::<ParameterId>(),
        TypeId::of::<ResourceId>(),
    ];
    for (index, left) in types.iter().enumerate() {
        for right in &types[index + 1..] {
            assert_ne!(left, right);
        }
    }

    assert_canonical::<GraphId>("graph");
    assert_canonical::<NodeId>("node");
    assert_canonical::<PortId>("port");
    assert_canonical::<EdgeId>("edge");
    assert_canonical::<ParameterId>("parameter");
    assert_canonical::<ResourceId>("resource");
}
