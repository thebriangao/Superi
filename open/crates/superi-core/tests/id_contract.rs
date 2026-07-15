use std::any::type_name;
use std::collections::BTreeSet;
use std::fmt::{Debug, Display};
use std::hash::Hash;
use std::str::FromStr;

use superi_core::ids::{
    BinId, CacheId, CaptionId, ClipId, DeviceId, EdgeId, GapId, GeneratorId, GraphId,
    IdentifierKind, JobId, MarkerId, MediaId, NodeId, ParameterId, ParseIdentifierError, PortId,
    ProjectId, ResourceId, SmartCollectionId, TimelineId, TrackId, TransitionId, TypedId,
};

const RAW: u128 = 0x0011_2233_4455_6677_8899_aabb_ccdd_eeff;
const BYTES: [u8; 16] = [
    0x00, 0x11, 0x22, 0x33, 0x44, 0x55, 0x66, 0x77, 0x88, 0x99, 0xaa, 0xbb, 0xcc, 0xdd, 0xee, 0xff,
];

fn assert_common_traits<T: Copy + Eq + Ord + Hash + Send + Sync + 'static>() {}

fn assert_contract<T>(kind: IdentifierKind, type_label: &str)
where
    T: TypedId + Copy + Debug + Display + Eq + FromStr<Err = ParseIdentifierError> + Hash + Ord,
{
    assert_eq!(T::KIND, kind);

    let id = T::from_raw(RAW);
    assert_eq!(id.raw(), RAW);
    assert_eq!(id.to_bytes(), BYTES);
    assert_eq!(T::from_bytes(BYTES), id);

    let text = format!("{}:00112233445566778899aabbccddeeff", kind.code());
    assert_eq!(id.to_string(), text);
    assert_eq!(format!("{id:?}"), format!("{type_label}({text})"));
    assert_eq!(text.parse::<T>(), Ok(id));
}

#[test]
fn concrete_types_are_distinct_and_share_one_contract() {
    assert_common_traits::<ProjectId>();
    assert_common_traits::<MediaId>();
    assert_common_traits::<BinId>();
    assert_common_traits::<SmartCollectionId>();
    assert_common_traits::<TrackId>();
    assert_common_traits::<ClipId>();
    assert_common_traits::<TimelineId>();
    assert_common_traits::<GapId>();
    assert_common_traits::<TransitionId>();
    assert_common_traits::<GeneratorId>();
    assert_common_traits::<CaptionId>();
    assert_common_traits::<MarkerId>();
    assert_common_traits::<NodeId>();
    assert_common_traits::<ParameterId>();
    assert_common_traits::<JobId>();
    assert_common_traits::<CacheId>();
    assert_common_traits::<DeviceId>();
    assert_common_traits::<GraphId>();
    assert_common_traits::<PortId>();
    assert_common_traits::<EdgeId>();
    assert_common_traits::<ResourceId>();

    assert_eq!(std::mem::size_of::<ProjectId>(), 16);
    assert_ne!(type_name::<ProjectId>(), type_name::<MediaId>());
    assert_ne!(type_name::<NodeId>(), type_name::<ParameterId>());
    assert_ne!(type_name::<GraphId>(), type_name::<NodeId>());
    assert_ne!(type_name::<PortId>(), type_name::<EdgeId>());
    assert_ne!(type_name::<ResourceId>(), type_name::<MediaId>());
    assert_ne!(type_name::<BinId>(), type_name::<SmartCollectionId>());
    assert_ne!(type_name::<TimelineId>(), type_name::<ProjectId>());
    assert_ne!(type_name::<GapId>(), type_name::<ClipId>());
    assert_ne!(type_name::<GeneratorId>(), type_name::<CaptionId>());

    assert_contract::<ProjectId>(IdentifierKind::Project, "ProjectId");
    assert_contract::<MediaId>(IdentifierKind::Media, "MediaId");
    assert_contract::<BinId>(IdentifierKind::Bin, "BinId");
    assert_contract::<SmartCollectionId>(IdentifierKind::SmartCollection, "SmartCollectionId");
    assert_contract::<TrackId>(IdentifierKind::Track, "TrackId");
    assert_contract::<ClipId>(IdentifierKind::Clip, "ClipId");
    assert_contract::<TimelineId>(IdentifierKind::Timeline, "TimelineId");
    assert_contract::<GapId>(IdentifierKind::Gap, "GapId");
    assert_contract::<TransitionId>(IdentifierKind::Transition, "TransitionId");
    assert_contract::<GeneratorId>(IdentifierKind::Generator, "GeneratorId");
    assert_contract::<CaptionId>(IdentifierKind::Caption, "CaptionId");
    assert_contract::<MarkerId>(IdentifierKind::Marker, "MarkerId");
    assert_contract::<NodeId>(IdentifierKind::Node, "NodeId");
    assert_contract::<ParameterId>(IdentifierKind::Parameter, "ParameterId");
    assert_contract::<JobId>(IdentifierKind::Job, "JobId");
    assert_contract::<CacheId>(IdentifierKind::Cache, "CacheId");
    assert_contract::<DeviceId>(IdentifierKind::Device, "DeviceId");
    assert_contract::<GraphId>(IdentifierKind::Graph, "GraphId");
    assert_contract::<PortId>(IdentifierKind::Port, "PortId");
    assert_contract::<EdgeId>(IdentifierKind::Edge, "EdgeId");
    assert_contract::<ResourceId>(IdentifierKind::Resource, "ResourceId");
}

#[test]
fn kind_codes_are_stable_and_discoverable() {
    let expected = [
        "project",
        "media",
        "bin",
        "smart_collection",
        "track",
        "clip",
        "timeline",
        "gap",
        "transition",
        "generator",
        "caption",
        "marker",
        "node",
        "parameter",
        "job",
        "cache",
        "device",
        "graph",
        "port",
        "edge",
        "resource",
    ];
    let actual: Vec<_> = IdentifierKind::ALL.iter().map(|kind| kind.code()).collect();
    assert_eq!(actual, expected);

    for kind in IdentifierKind::ALL {
        assert_eq!(IdentifierKind::from_code(kind.code()), Some(*kind));
        assert_eq!(kind.to_string(), kind.code());
    }
    assert_eq!(IdentifierKind::from_code("extension"), None);
}

#[test]
fn values_have_platform_independent_bytes_and_ordering() {
    assert_eq!(ProjectId::from_raw(RAW).to_bytes(), BYTES);
    assert_eq!(ProjectId::from_bytes(BYTES).raw(), RAW);
    assert_eq!(
        ProjectId::from_raw(0).to_string(),
        "project:00000000000000000000000000000000"
    );
    assert_eq!(
        ProjectId::from_raw(u128::MAX).to_string(),
        "project:ffffffffffffffffffffffffffffffff"
    );

    let values = BTreeSet::from([
        NodeId::from_raw(9),
        NodeId::from_raw(1),
        NodeId::from_raw(4),
    ]);
    let ordered: Vec<_> = values.into_iter().map(NodeId::raw).collect();
    assert_eq!(ordered, [1, 4, 9]);
}

#[test]
fn parser_rejects_wrong_identifier_domains() {
    let error = "media:00112233445566778899aabbccddeeff"
        .parse::<ProjectId>()
        .expect_err("wrong domain must fail");
    assert_eq!(
        error,
        ParseIdentifierError::UnexpectedKind {
            expected: IdentifierKind::Project,
            actual: IdentifierKind::Media,
        }
    );
    assert_eq!(
        error.to_string(),
        "expected project identifier, found media identifier"
    );

    assert_eq!(
        "extension:00112233445566778899aabbccddeeff".parse::<ProjectId>(),
        Err(ParseIdentifierError::UnknownKind)
    );
}

#[test]
fn parser_accepts_only_the_canonical_text() {
    assert_eq!(
        "project00112233445566778899aabbccddeeff".parse::<ProjectId>(),
        Err(ParseIdentifierError::MissingSeparator)
    );
    assert_eq!(
        "project:1234".parse::<ProjectId>(),
        Err(ParseIdentifierError::InvalidLength {
            expected: 32,
            actual: 4,
        })
    );
    assert_eq!(
        "project:00112233445566778899aabbccddeezz".parse::<ProjectId>(),
        Err(ParseIdentifierError::InvalidHex { index: 30 })
    );
    assert_eq!(
        "project:00112233445566778899AABBCCDDEEFF".parse::<ProjectId>(),
        Err(ParseIdentifierError::InvalidHex { index: 20 })
    );
    assert!(" project:00112233445566778899aabbccddeeff"
        .parse::<ProjectId>()
        .is_err());
    assert!("project:00112233445566778899aabbccddeeff\n"
        .parse::<ProjectId>()
        .is_err());
}

#[test]
fn parse_errors_are_thread_safe_standard_errors() {
    fn assert_error<T: std::error::Error + Send + Sync + 'static>() {}

    assert_error::<ParseIdentifierError>();
}
