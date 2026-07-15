use std::collections::BTreeMap;
use std::str::FromStr;
use std::sync::Arc;

use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::settings::{CapabilitySet, SemanticVersion};
use superi_core::time::{RationalTime, Timebase};
use superi_effects::authoring::{
    EffectInstanceBindings, EffectMetadata, EffectNodeDefinition, EffectParameterBinding,
    EffectParameterDefinition, ParameterControl,
};
use superi_effects::control::{ControlRelationship, ParameterControlRig, ReusableControl};
use superi_effects::keyframe::{
    AnimationCurve, AnimationValue, Easing, Interpolation, Keyframe, KeyframeTiming,
};
use superi_effects::text::{
    FontFace, FontResolver, OpenTypeFeature, ParagraphSpan, ParagraphStyle, TextAlignment,
    TextDirection, TextLayer, TextLayoutEngine, TextRange, TextStyle, TextStyleSpan, TextWrap,
    VariationAxis, TEXT_LAYER_SCHEMA_REVISION,
};
use superi_graph::expr::{ParameterAddress, ParameterReference};
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

fn clock() -> Timebase {
    Timebase::integer(24).unwrap()
}

fn at(value: i64) -> RationalTime {
    RationalTime::new(value, clock())
}

fn animated(start: &[f64], end: &[f64], interpolation: Interpolation) -> AnimationCurve {
    AnimationCurve::new(
        clock(),
        [
            Keyframe::new(
                KeyframeTiming::Fixed(at(0)),
                AnimationValue::new(start.iter().copied()).unwrap(),
                interpolation,
                Easing::Linear,
                None,
                None,
            )
            .unwrap(),
            Keyframe::new(
                KeyframeTiming::Fixed(at(10)),
                AnimationValue::new(end.iter().copied()).unwrap(),
                Interpolation::Hold,
                Easing::Linear,
                None,
                None,
            )
            .unwrap(),
        ],
        None,
    )
    .unwrap()
}

fn fixed(value: &[f64]) -> AnimationCurve {
    animated(value, value, Interpolation::Linear)
}

fn face() -> FontFace {
    FontFace::new("font.tinos.subset", "Tinos", "Tinos Subset", 0).unwrap()
}

fn style(size_start: f64, size_end: f64) -> TextStyle {
    TextStyle::new(
        face(),
        animated(&[size_start], &[size_end], Interpolation::Linear),
        fixed(&[0.25, 0.5, 0.75, 1.0]),
        fixed(&[0.8]),
        fixed(&[0.0]),
        fixed(&[0.0]),
        [OpenTypeFeature::new(*b"kern", 1).unwrap()],
        [VariationAxis::new(*b"wght", fixed(&[400.0])).unwrap()],
    )
    .unwrap()
}

fn paragraph(width_start: f64, width_end: f64) -> ParagraphStyle {
    ParagraphStyle::new(
        animated(&[width_start], &[width_end], Interpolation::Linear),
        fixed(&[30.0]),
        fixed(&[4.0]),
        fixed(&[3.0]),
        fixed(&[5.0]),
        fixed(&[2.0]),
        fixed(&[6.0]),
        animated(
            &[TextAlignment::Start.code()],
            &[TextAlignment::End.code()],
            Interpolation::Hold,
        ),
        fixed(&[TextDirection::Auto.code()]),
        fixed(&[TextWrap::Word.code()]),
    )
    .unwrap()
}

fn layer(text: &str, width_start: f64, width_end: f64) -> TextLayer {
    let range = TextRange::new(0, text.len()).unwrap();
    TextLayer::new(
        text,
        [TextStyleSpan::new(range, style(20.0, 28.0)).unwrap()],
        [ParagraphSpan::new(range, paragraph(width_start, width_end)).unwrap()],
    )
    .unwrap()
}

#[derive(Default)]
struct TestFonts {
    fonts: BTreeMap<String, Arc<[u8]>>,
}

impl TestFonts {
    fn with_tinos() -> Self {
        Self {
            fonts: BTreeMap::from([(
                "font.tinos.subset".to_owned(),
                Arc::<[u8]>::from(font_test_data::TINOS_SUBSET),
            )]),
        }
    }
}

impl FontResolver for TestFonts {
    fn resolve(&self, font: &FontFace) -> Result<Arc<[u8]>> {
        self.fonts.get(font.asset_id()).cloned().ok_or_else(|| {
            Error::new(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "font asset is unavailable",
            )
            .with_context(
                ErrorContext::new("text-contract", "resolve_font")
                    .with_field("asset_id", font.asset_id()),
            )
        })
    }
}

#[test]
fn real_font_bytes_shape_into_inspectable_deterministic_glyph_runs() {
    let fonts = TestFonts::with_tinos();
    let layer = layer("aAbB aB", 300.0, 300.0);
    let mut engine = TextLayoutEngine::new();

    let first = engine.layout(&layer, at(0), &fonts).unwrap();
    let second = engine.layout(&layer, at(0), &fonts).unwrap();
    assert_eq!(first, second);
    assert_eq!(first.lines().len(), 1);
    assert!(first.width() > 0.0);
    assert!(first.height() > 0.0);

    let line = &first.lines()[0];
    assert_eq!(line.source_range(), TextRange::new(0, 7).unwrap());
    assert!(!line.runs().is_empty());
    let glyphs = line
        .runs()
        .iter()
        .flat_map(|run| run.glyphs())
        .collect::<Vec<_>>();
    assert!(glyphs.len() >= 6);
    assert!(glyphs.iter().any(|glyph| glyph.glyph_id() != 0));
    assert!(glyphs.windows(2).all(|pair| pair[0].x() <= pair[1].x()));
    assert!(glyphs.iter().all(|glyph| {
        let range = glyph.source_range();
        range.start() < range.end()
            && layer.text().is_char_boundary(range.start())
            && layer.text().is_char_boundary(range.end())
            && glyph.advance().is_finite()
    }));
    assert_eq!(line.runs()[0].font().asset_id(), "font.tinos.subset");
    assert_eq!(line.runs()[0].fill_rgba(), [0.25, 0.5, 0.75, 1.0]);
    assert_eq!(line.runs()[0].opacity(), 0.8);
}

#[test]
fn unicode_bidi_and_paragraph_controls_remain_visual_and_inspectable() {
    let fonts = TestFonts::with_tinos();
    let text = "a אב b";
    let layer = layer(text, 240.0, 240.0);
    let mut engine = TextLayoutEngine::new();
    let layout = engine.layout(&layer, at(0), &fonts).unwrap();

    assert_eq!(layout.lines().len(), 1);
    assert!(layout.lines()[0]
        .runs()
        .iter()
        .any(|run| run.direction() == TextDirection::RightToLeft));
    assert!(layout.lines()[0]
        .runs()
        .iter()
        .any(|run| run.direction() == TextDirection::LeftToRight));
    assert_eq!(layout.lines()[0].paragraph_index(), 0);
    assert_eq!(layout.lines()[0].line_index_in_paragraph(), 0);
    assert!(layout.lines()[0].origin_x() >= 7.0);
}

#[test]
fn animated_wrapping_alignment_and_typography_retime_as_one_editable_layer() {
    let fonts = TestFonts::with_tinos();
    let layer = layer("aB aB aB aB aB", 180.0, 68.0);
    let mut engine = TextLayoutEngine::new();

    let wide = engine.layout(&layer, at(0), &fonts).unwrap();
    let narrow = engine.layout(&layer, at(10), &fonts).unwrap();
    assert!(narrow.lines().len() > wide.lines().len());
    assert!(narrow.lines()[0].origin_x() > 3.0);
    assert_eq!(
        layer.style_spans()[0]
            .style()
            .font_size()
            .evaluate(at(10))
            .unwrap()
            .components(),
        &[28.0]
    );

    let retimed = layer.retimed(at(100), at(120)).unwrap();
    assert_eq!(retimed.timebase(), clock());
    assert_eq!(
        retimed.style_spans()[0]
            .style()
            .font_size()
            .evaluate(at(120))
            .unwrap(),
        layer.style_spans()[0]
            .style()
            .font_size()
            .evaluate(at(10))
            .unwrap()
    );
    assert_eq!(
        retimed.paragraph_spans()[0]
            .style()
            .width()
            .evaluate(at(120))
            .unwrap(),
        layer.paragraph_spans()[0]
            .style()
            .width()
            .evaluate(at(10))
            .unwrap()
    );
    let retimed_layout = engine.layout(&retimed, at(120), &fonts).unwrap();
    assert_eq!(retimed_layout.lines().len(), narrow.lines().len());
    assert_eq!(retimed_layout.width(), narrow.width());
    assert_eq!(
        retimed_layout.lines()[0].origin_x(),
        narrow.lines()[0].origin_x()
    );
}

#[test]
fn utf8_text_and_style_edits_are_immutable_checked_and_directly_inspectable() {
    let initial = layer("aéB", 240.0, 240.0);
    let edited = initial
        .with_replaced_text(TextRange::new(1, 3).unwrap(), "Ab")
        .unwrap();
    assert_eq!(initial.text(), "aéB");
    assert_eq!(edited.text(), "aAbB");

    let alternate = TextStyle::new(
        face(),
        fixed(&[32.0]),
        fixed(&[1.0, 0.0, 0.0, 1.0]),
        fixed(&[1.0]),
        fixed(&[1.5]),
        fixed(&[2.0]),
        [],
        [],
    )
    .unwrap();
    let restyled = edited
        .with_style(TextRange::new(1, 3).unwrap(), alternate.clone())
        .unwrap();
    assert_eq!(restyled.style_spans().len(), 3);
    assert_eq!(
        restyled.style_spans()[1].range(),
        TextRange::new(1, 3).unwrap()
    );
    assert_eq!(restyled.style_spans()[1].style(), &alternate);
    assert_ne!(edited, restyled);

    let paragraph = ParagraphStyle::new(
        fixed(&[280.0]),
        fixed(&[36.0]),
        fixed(&[0.0]),
        fixed(&[0.0]),
        fixed(&[0.0]),
        fixed(&[10.0]),
        fixed(&[12.0]),
        fixed(&[TextAlignment::Center.code()]),
        fixed(&[TextDirection::LeftToRight.code()]),
        fixed(&[TextWrap::Anywhere.code()]),
    )
    .unwrap();
    let reparagraphed = restyled
        .with_paragraph_style(TextRange::new(0, 4).unwrap(), paragraph.clone())
        .unwrap();
    assert_eq!(reparagraphed.paragraph_spans()[0].style(), &paragraph);
    assert!(initial
        .with_replaced_text(TextRange::new(2, 3).unwrap(), "x")
        .is_err());
}

#[test]
fn text_wire_is_strict_versioned_and_revalidates_nested_state() {
    let layer = layer("aAbB", 200.0, 200.0);
    let document = serde_json::to_value(&layer).unwrap();
    assert_eq!(document["schema_revision"], TEXT_LAYER_SCHEMA_REVISION);
    assert_eq!(
        serde_json::from_value::<TextLayer>(document.clone()).unwrap(),
        layer
    );

    let mut future = document.clone();
    future["schema_revision"] = (TEXT_LAYER_SCHEMA_REVISION + 1).into();
    assert!(serde_json::from_value::<TextLayer>(future).is_err());

    let mut unknown = document.clone();
    unknown["unexpected"] = true.into();
    assert!(serde_json::from_value::<TextLayer>(unknown).is_err());

    let mut invalid_range = document.clone();
    invalid_range["style_spans"][0]["range"]["end"] = 999.into();
    assert!(serde_json::from_value::<TextLayer>(invalid_range).is_err());

    let mut invalid_sample = document;
    invalid_sample["paragraph_spans"][0]["style"]["alignment"]["keyframes"][0]["value"][0] =
        0.5.into();
    assert!(serde_json::from_value::<TextLayer>(invalid_sample).is_err());
}

#[test]
fn layout_fails_before_publication_for_missing_font_or_invalid_sampled_controls() {
    let layer = layer("aB", 200.0, 200.0);
    let mut engine = TextLayoutEngine::new();
    let missing = engine
        .layout(&layer, at(0), &TestFonts::default())
        .unwrap_err();
    assert_eq!(missing.category(), ErrorCategory::NotFound);

    let invalid_width = ParagraphStyle::new(
        animated(&[200.0], &[-1.0], Interpolation::Linear),
        fixed(&[30.0]),
        fixed(&[0.0]),
        fixed(&[0.0]),
        fixed(&[0.0]),
        fixed(&[0.0]),
        fixed(&[0.0]),
        fixed(&[TextAlignment::Start.code()]),
        fixed(&[TextDirection::Auto.code()]),
        fixed(&[TextWrap::Word.code()]),
    )
    .unwrap_err();
    assert_eq!(invalid_width.category(), ErrorCategory::InvalidInput);
}

fn value_type(input: &str) -> ValueTypeId {
    ValueTypeId::from_str(input).unwrap()
}

fn parameter(input: &str) -> ParameterName {
    ParameterName::new(input).unwrap()
}

fn text_node(layer: TextLayer, parameter_id: ParameterId) -> EditableNode<GraphValue<TextLayer>> {
    let text_type = value_type("superi.value.text_layer");
    let definition = EffectNodeDefinition::new(
        NodeSchemaId::new(
            NodeTypeId::from_str("superi.effects.text_layer").unwrap(),
            SemanticVersion::new(1, 0, 0),
        ),
        EffectMetadata::new(
            "Text Layer",
            "Stores one reusable editable text layout artifact.",
            "Text",
        )
        .unwrap(),
        [],
        [],
        [EffectParameterDefinition::new(
            ParameterSchema::new(parameter("text"), text_type.clone(), true),
            "Text",
            "Editable styled text, paragraphs, and animation.",
            ParameterControl::Automatic,
            TypedParameterValue::new(text_type, GraphValue::domain(layer)),
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
    definition
        .instantiate(
            EffectInstanceBindings::new(
                [],
                [],
                [EffectParameterBinding::new(parameter_id, parameter("text"))],
            ),
            [],
        )
        .unwrap()
}

#[test]
fn one_text_payload_reuses_across_timeline_and_node_graph_roles_and_reload() {
    let initial = layer("aB aB", 220.0, 220.0);
    let edited = initial
        .with_replaced_text(TextRange::new(3, 5).unwrap(), "aAbB")
        .unwrap();
    let text_type = value_type("superi.value.text_layer");
    let fonts = TestFonts::with_tinos();

    for (graph_id, source_node, source_parameter, target_node, target_parameter) in [
        (
            GraphId::from_raw(700),
            NodeId::from_raw(701),
            ParameterId::from_raw(702),
            NodeId::from_raw(703),
            ParameterId::from_raw(704),
        ),
        (
            GraphId::from_raw(800),
            NodeId::from_raw(801),
            ParameterId::from_raw(802),
            NodeId::from_raw(803),
            ParameterId::from_raw(804),
        ),
    ] {
        let mut graph = EditableGraph::new(graph_id);
        graph
            .apply(GraphTransaction::with_mutations(
                0,
                [
                    GraphMutation::Add {
                        node_id: source_node,
                        node: text_node(initial.clone(), source_parameter),
                        position: 0,
                    },
                    GraphMutation::Add {
                        node_id: target_node,
                        node: text_node(initial.clone(), target_parameter),
                        position: 1,
                    },
                ],
            ))
            .unwrap();
        graph
            .apply(GraphTransaction::with_mutations(
                1,
                [GraphMutation::SetParameter {
                    node_id: source_node,
                    parameter_id: source_parameter,
                    value: TypedParameterValue::new(
                        text_type.clone(),
                        GraphValue::domain(edited.clone()),
                    ),
                }],
            ))
            .unwrap();

        let source = ParameterAddress::new(source_node, source_parameter);
        let target = ParameterAddress::new(target_node, target_parameter);
        let control_name = parameter("shared-text");
        let rig = ParameterControlRig::new(
            [ReusableControl::new(
                control_name.clone(),
                "Shared Text",
                "Reuses the complete text authoring artifact.",
                ParameterControl::Automatic,
                ParameterReference::new(source, text_type.clone()),
            )
            .unwrap()],
            [ControlRelationship::link(target, control_name)],
        )
        .unwrap();
        graph
            .apply(rig.transaction(&graph.snapshot()).unwrap())
            .unwrap();

        let expected = graph
            .snapshot()
            .evaluate_parameter(target)
            .unwrap()
            .value()
            .payload()
            .as_domain()
            .unwrap()
            .clone();
        assert_eq!(expected, edited);
        let bytes = serialize_graph(&graph.snapshot()).unwrap();
        let loaded = deserialize_graph::<GraphValue<TextLayer>>(&bytes).unwrap();
        let loaded_snapshot = loaded.graph().snapshot();
        let reloaded_evaluation = loaded_snapshot.evaluate_parameter(target).unwrap();
        let reloaded = reloaded_evaluation.value().payload().as_domain().unwrap();
        assert_eq!(reloaded, &edited);
        assert_eq!(serialize_graph(&loaded_snapshot).unwrap(), bytes);

        let mut engine = TextLayoutEngine::new();
        assert_eq!(
            engine.layout(reloaded, at(5), &fonts).unwrap(),
            engine.layout(&edited, at(5), &fonts).unwrap()
        );
    }
}
