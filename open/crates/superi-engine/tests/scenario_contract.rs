use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use sha2::{Digest, Sha256};
use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::time::FrameRate;
use superi_engine::command::{
    GraphEdgeMode, GraphEffect, GraphNodeKind, GraphSampling, ScenarioAction, ScenarioEngine,
    ScenarioPhase, SliceImplementation, CANONICAL_FIXTURE_ID, CANONICAL_FIXTURE_VERSION,
    CANONICAL_HEIGHT, CANONICAL_SOURCE_FRAMES, CANONICAL_WIDTH, MAX_SCENARIO_SOURCE_BYTES,
};

#[test]
fn canonical_actions_create_the_exact_timeline_graph_and_operation_log() {
    let directory = test_directory("canonical");
    let source = directory.join("input.webm");
    let source_bytes = b"deterministic fixture stand-in for the engine contract";
    fs::write(&source, source_bytes).unwrap();
    let mut engine = ScenarioEngine::new();

    let imported = engine
        .execute(import_action(source.clone(), source_bytes))
        .unwrap();
    assert_eq!(imported.phase(), ScenarioPhase::Imported);
    assert_eq!(imported.revision(), 1);
    let media = imported.media().unwrap();
    assert_eq!(media.fixture_id(), CANONICAL_FIXTURE_ID);
    assert_eq!(media.fixture_version(), CANONICAL_FIXTURE_VERSION);
    assert_eq!(media.path(), source);
    assert_eq!(media.frame_rate(), FrameRate::FPS_24);
    assert_eq!(media.frame_count(), CANONICAL_SOURCE_FRAMES);
    assert_eq!(media.width(), CANONICAL_WIDTH);
    assert_eq!(media.height(), CANONICAL_HEIGHT);
    assert_eq!(media.implementation(), SliceImplementation::Reference);

    let placed = engine
        .execute(ScenarioAction::PlaceClip {
            timeline_start_frame: 0,
        })
        .unwrap();
    let timeline = placed.timeline().unwrap();
    assert_eq!(placed.phase(), ScenarioPhase::Placed);
    assert_eq!(timeline.timeline_name(), "canonical");
    assert_eq!(timeline.track_name(), "V1");
    assert_eq!(timeline.clip_name(), "clip-1");
    assert_eq!(timeline.edit_rate(), FrameRate::FPS_24);
    assert_eq!(timeline.canvas(), (CANONICAL_WIDTH, CANONICAL_HEIGHT));
    assert_eq!(timeline.source_range(), (0, 96));
    assert_eq!(timeline.timeline_range(), (0, 96));

    let trimmed = engine
        .execute(ScenarioAction::TrimClip {
            source_start_frame: 24,
            source_end_frame: 72,
        })
        .unwrap();
    assert_eq!(trimmed.phase(), ScenarioPhase::Trimmed);
    assert_eq!(trimmed.timeline().unwrap().source_range(), (24, 72));
    assert_eq!(trimmed.timeline().unwrap().timeline_range(), (0, 48));

    let final_state = engine
        .execute(ScenarioAction::ApplyGraphEffect {
            effect: GraphEffect::HorizontalMirror,
        })
        .unwrap();
    assert_eq!(final_state.phase(), ScenarioPhase::Effected);
    assert_eq!(final_state.revision(), 4);
    assert_eq!(final_state.undo_depth(), 4);
    assert_eq!(final_state.redo_depth(), 0);
    let graph = final_state.graph().unwrap();
    assert_eq!(graph.nodes().len(), 3);
    assert_eq!(graph.edges().len(), 2);
    assert_eq!(graph.nodes()[0].kind(), GraphNodeKind::Source);
    assert_eq!(graph.nodes()[0].instance_id(), "slice.node.source");
    assert_eq!(graph.nodes()[1].kind(), GraphNodeKind::Effect);
    assert_eq!(graph.nodes()[1].instance_id(), "slice.node.effect");
    assert_eq!(graph.nodes()[1].node_type(), "superi.effect.transform");
    assert_eq!(graph.nodes()[1].schema_version(), 1);
    assert_eq!(graph.nodes()[2].kind(), GraphNodeKind::Output);
    assert_eq!(graph.nodes()[2].instance_id(), "slice.node.output");
    assert_eq!(graph.edges()[0].from_node(), "slice.node.source");
    assert_eq!(graph.edges()[0].to_node(), "slice.node.effect");
    assert_eq!(graph.edges()[1].from_node(), "slice.node.effect");
    assert_eq!(graph.edges()[1].to_node(), "slice.node.output");
    assert_eq!(
        graph.matrix(),
        [-1.0, 0.0, 95.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0]
    );
    assert_eq!(graph.sampling(), GraphSampling::Nearest);
    assert_eq!(graph.edge_mode(), GraphEdgeMode::TransparentBlack);
    assert_eq!(graph.output_extent(), (CANONICAL_WIDTH, CANONICAL_HEIGHT));
    assert_eq!(graph.derived_timeline_identity(), "canonical/V1/clip-1");

    let operation_ids = final_state
        .operation_log()
        .iter()
        .map(|operation| operation.operation_id())
        .collect::<Vec<_>>();
    assert_eq!(
        operation_ids,
        [
            "slice.op.import",
            "slice.op.insert",
            "slice.op.trim",
            "slice.op.effect"
        ]
    );
    assert_eq!(
        final_state
            .operation_log()
            .iter()
            .map(|operation| operation.resulting_revision())
            .collect::<Vec<_>>(),
        [1, 2, 3, 4]
    );

    engine.execute(ScenarioAction::Undo).unwrap();
    let inverse = engine.execute(ScenarioAction::Undo).unwrap();
    assert_eq!(inverse.phase(), ScenarioPhase::Placed);
    assert_eq!(inverse.operation_log().len(), 2);
    engine.execute(ScenarioAction::Redo).unwrap();
    let replayed = engine.execute(ScenarioAction::Redo).unwrap();
    assert_eq!(replayed.phase(), ScenarioPhase::Effected);
    assert_eq!(replayed.revision(), 8);
    assert_eq!(replayed.media(), final_state.media());
    assert_eq!(replayed.timeline(), final_state.timeline());
    assert_eq!(replayed.graph(), final_state.graph());
    assert_eq!(replayed.operation_log(), final_state.operation_log());

    engine.execute(ScenarioAction::Undo).unwrap();
    let branched = engine
        .execute(ScenarioAction::ApplyGraphEffect {
            effect: GraphEffect::HorizontalMirror,
        })
        .unwrap();
    assert_eq!(branched.revision(), 10);
    assert_eq!(branched.undo_depth(), 4);
    assert_eq!(branched.redo_depth(), 0);
    let before_empty_redo = engine.snapshot();
    assert_eq!(
        engine.execute(ScenarioAction::Redo).unwrap_err().category(),
        ErrorCategory::Conflict
    );
    assert_eq!(engine.snapshot(), before_empty_redo);
}

#[test]
fn canonical_arguments_are_exact_and_rejected_actions_are_atomic() {
    let directory = test_directory("exact");
    let source = directory.join("input.webm");
    let source_bytes = b"source";
    fs::write(&source, source_bytes).unwrap();
    let mut engine = ScenarioEngine::new();
    engine.execute(import_action(source, source_bytes)).unwrap();
    let before = engine.snapshot();

    let error = engine
        .execute(ScenarioAction::PlaceClip {
            timeline_start_frame: 1,
        })
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(engine.snapshot(), before);

    engine
        .execute(ScenarioAction::PlaceClip {
            timeline_start_frame: 0,
        })
        .unwrap();
    let before = engine.snapshot();
    let error = engine
        .execute(ScenarioAction::TrimClip {
            source_start_frame: 23,
            source_end_frame: 72,
        })
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(engine.snapshot(), before);
}

#[test]
fn import_validates_the_payload_digest_and_responsiveness_bound() {
    let directory = test_directory("identity");
    let source = directory.join("input.webm");
    fs::write(&source, b"source").unwrap();
    let mut engine = ScenarioEngine::new();
    let mut action = import_action(source.clone(), b"different bytes");

    let error = engine.execute(action.clone()).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::CorruptData);
    assert_eq!(engine.snapshot().phase(), ScenarioPhase::Empty);

    let file = fs::File::create(&source).unwrap();
    file.set_len(MAX_SCENARIO_SOURCE_BYTES + 1).unwrap();
    action = import_action(source, b"ignored");
    let error = engine.execute(action).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
    assert_eq!(engine.snapshot().revision(), 0);
}

fn import_action(path: PathBuf, payload: &[u8]) -> ScenarioAction {
    ScenarioAction::ImportClip {
        path,
        fixture_id: CANONICAL_FIXTURE_ID.to_owned(),
        fixture_version: CANONICAL_FIXTURE_VERSION,
        manifest_sha256: "1d2b28b5f44c7f86dce50d67b718b0fad967d267d9016961e3d71bb9dab94419"
            .to_owned(),
        payload_sha256: sha256(payload),
        frame_rate: FrameRate::FPS_24,
        frame_count: CANONICAL_SOURCE_FRAMES,
        width: CANONICAL_WIDTH,
        height: CANONICAL_HEIGHT,
    }
}

fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn test_directory(label: &str) -> PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "superi-p1-w07-c017-{label}-{}-{nonce}",
        std::process::id()
    ));
    fs::create_dir_all(&path).unwrap();
    path
}
