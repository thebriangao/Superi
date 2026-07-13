use std::str::FromStr;

use superi_core::error::{ErrorCategory, Recoverability};
use superi_image::channels::{ChannelIndex, ChannelList, ChannelName, LayerName, StandardChannel};

fn names<'a>(channels: impl Iterator<Item = &'a ChannelName>) -> Vec<&'a str> {
    channels.map(ChannelName::as_str).collect()
}

#[test]
fn qualified_names_preserve_exact_nested_layer_and_channel_identity() {
    let channel = ChannelName::from_str("lighting.key.specular.R").unwrap();

    assert_eq!(channel.as_str(), "lighting.key.specular.R");
    assert_eq!(channel.base_name(), "R");
    assert_eq!(channel.layer().as_str(), "lighting.key.specular");
    assert_eq!(channel.layer().depth(), 3);
    assert_eq!(channel.to_string(), "lighting.key.specular.R");
    assert_eq!(channel.standard(), Some(StandardChannel::Red));

    let parent = channel.layer().parent().unwrap();
    assert_eq!(parent.as_str(), "lighting.key");
    assert!(parent.encloses(channel.layer()));
    assert!(parent.directly_encloses(channel.layer()));
    assert!(LayerName::base().encloses(channel.layer()));
    assert!(!channel.layer().encloses(&parent));
}

#[test]
fn base_layers_and_structured_construction_are_unambiguous() {
    let base = ChannelName::new("A").unwrap();
    assert!(base.layer().is_base());
    assert_eq!(base.as_str(), "A");
    assert_eq!(base.standard(), Some(StandardChannel::Alpha));

    let beauty = LayerName::from_str("beauty.diffuse").unwrap();
    let green = ChannelName::in_layer(beauty.clone(), "G").unwrap();
    assert_eq!(green.as_str(), "beauty.diffuse.G");
    assert_eq!(green.layer(), &beauty);
    assert_eq!(
        beauty.child("indirect").unwrap().as_str(),
        "beauty.diffuse.indirect"
    );
    assert_eq!(
        beauty.components().collect::<Vec<_>>(),
        ["beauty", "diffuse"]
    );
}

#[test]
fn standard_meanings_use_the_base_name_without_rewriting_custom_names() {
    let cases = [
        ("R", Some(StandardChannel::Red)),
        ("rgba.G", Some(StandardChannel::Green)),
        ("B", Some(StandardChannel::Blue)),
        ("A", Some(StandardChannel::Alpha)),
        ("Y", Some(StandardChannel::Luminance)),
        ("RY", Some(StandardChannel::RedChroma)),
        ("BY", Some(StandardChannel::BlueChroma)),
        ("AR", Some(StandardChannel::RedAlpha)),
        ("AG", Some(StandardChannel::GreenAlpha)),
        ("AB", Some(StandardChannel::BlueAlpha)),
        ("Z", Some(StandardChannel::Depth)),
        ("ZBack", Some(StandardChannel::BackDepth)),
        ("id", Some(StandardChannel::ObjectId)),
        ("motion.x", None),
        ("r", None),
    ];

    for (name, expected) in cases {
        let channel = ChannelName::from_str(name).unwrap();
        assert_eq!(
            channel.standard(),
            expected,
            "unexpected meaning for {name}"
        );
        assert_eq!(channel.as_str(), name);
    }
}

#[test]
fn channel_lists_keep_source_order_and_derive_nested_layers_deterministically() {
    let channels = ChannelList::from_full_names([
        "R",
        "lighting.diffuse.R",
        "lighting.diffuse.G",
        "lighting.specular.R",
        "crypto.object.id",
        "A",
    ])
    .unwrap();

    assert_eq!(channels.len(), 6);
    assert_eq!(channels[ChannelIndex::new(0)].as_str(), "R");
    assert_eq!(
        channels.index_of("lighting.specular.R"),
        Some(ChannelIndex::new(3))
    );
    assert_eq!(channels.index_of("missing"), None);
    assert_eq!(
        channels
            .layers()
            .iter()
            .map(LayerName::as_str)
            .collect::<Vec<_>>(),
        [
            "",
            "lighting",
            "lighting.diffuse",
            "lighting.specular",
            "crypto",
            "crypto.object"
        ]
    );

    let lighting = LayerName::from_str("lighting").unwrap();
    let diffuse = LayerName::from_str("lighting.diffuse").unwrap();
    assert!(channels.channels_in_layer(&lighting).next().is_none());
    assert_eq!(
        names(channels.channels_in_layer(&diffuse)),
        ["lighting.diffuse.R", "lighting.diffuse.G"]
    );
    assert_eq!(
        names(channels.channels_in_layer_tree(&lighting)),
        [
            "lighting.diffuse.R",
            "lighting.diffuse.G",
            "lighting.specular.R"
        ]
    );
    assert_eq!(
        names(channels.channels_in_layer(&LayerName::base())),
        ["R", "A"]
    );
}

#[test]
fn name_mapping_preserves_identity_when_storage_order_changes_or_channels_are_selected() {
    let source =
        ChannelList::from_full_names(["beauty.R", "beauty.G", "beauty.B", "depth.Z"]).unwrap();

    let indices = source
        .resolve_indices(["depth.Z", "beauty.B", "beauty.R"])
        .unwrap();
    assert_eq!(
        indices.iter().map(|index| index.get()).collect::<Vec<_>>(),
        [3, 2, 0]
    );
    assert_eq!(source[indices[0]].as_str(), "depth.Z");
    assert_eq!(source[indices[1]].as_str(), "beauty.B");
    assert_eq!(source[indices[2]].as_str(), "beauty.R");

    let error = source
        .resolve_indices(["beauty.R", "missing.X"])
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::NotFound);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(error.contexts()[0].operation(), "resolve_channel_indices");
    assert_eq!(error.contexts()[0].field("channel"), Some("missing.X"));
}

#[test]
fn names_remain_case_sensitive_unicode_and_lossless() {
    let original = [
        "反射.赤",
        "反射.赤色",
        "matte.ID",
        "matte.id",
        "custom channel",
        ".R",
        "odd.",
        "odd..R",
    ];
    let channels = ChannelList::from_full_names(original).unwrap();

    assert_eq!(
        channels.iter().map(ChannelName::as_str).collect::<Vec<_>>(),
        original
    );
    assert_ne!(channels.index_of("matte.ID"), channels.index_of("matte.id"));
    assert_eq!(channels.index_of("MATTE.ID"), None);

    for name in [".R", "odd.", "odd..R"] {
        let channel = &channels[channels.index_of(name).unwrap()];
        assert_eq!(channel.as_str(), name);
        assert_eq!(channel.base_name(), name);
        assert!(channel.layer().is_base());
    }
}

#[test]
fn malformed_and_duplicate_names_fail_with_actionable_shared_errors() {
    for invalid in ["", "bad\0name"] {
        let error = ChannelName::from_str(invalid).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::InvalidInput);
        assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
        assert_eq!(error.contexts()[0].component(), "superi-image.channels");
        assert_eq!(error.contexts()[0].operation(), "parse_channel_name");
    }

    for invalid in [".beauty", "beauty..diffuse", "beauty.", "bad\0layer"] {
        let error = LayerName::from_str(invalid).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::InvalidInput);
        assert_eq!(error.contexts()[0].operation(), "parse_layer_name");
    }

    let empty = ChannelList::new([]).unwrap_err();
    assert_eq!(empty.category(), ErrorCategory::InvalidInput);

    let duplicate = ChannelList::from_full_names(["R", "lighting.R", "R"]).unwrap_err();
    assert_eq!(duplicate.category(), ErrorCategory::InvalidInput);
    assert_eq!(duplicate.contexts()[0].operation(), "create_channel_list");
    assert_eq!(duplicate.contexts()[0].field("channel"), Some("R"));
}

#[test]
fn public_naming_values_are_safe_to_share_across_image_workers() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<ChannelIndex>();
    assert_send_sync::<LayerName>();
    assert_send_sync::<ChannelName>();
    assert_send_sync::<ChannelList>();
}
