use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use rusqlite::Connection;
use superi_audio::mixing::{ChannelMap, ClipMixControls, ClipMixMutation};
use superi_core::ids::{ClipId, GraphId, MediaId, ProjectId, TimelineId, TrackId};
use superi_core::pixel::{ChannelLayout, ChannelPosition};
use superi_core::settings::{ComponentId, SemanticVersion, SettingValue, VersionIdentifier};
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
use superi_graph::mutate::EditableGraph;
use superi_project::document::{ProjectDocument, ProjectGraph, StandaloneProjectGraph};
use superi_project::extensions::{
    ProjectExtensionCommand, ProjectExtensionKind, ProjectExtensionLifecycle,
    ProjectExtensionRecord, ProjectExtensionRecordId,
};
use superi_project::media::{PortableRelativePath, ProjectMediaCommand, ReferencedMediaPath};
use superi_project::settings::{
    ProjectSettingMutation, ProjectSettingsTransaction, AUDIO_SAMPLE_RATE_KEY,
};
use superi_project::{
    ProjectDatabase, ProjectDiagnosticComponent, ProjectDiagnostics, PROJECT_HASH_ALGORITHM,
    PROJECT_HASH_FORMAT_REVISION,
};
use superi_timeline::compile::CompiledTimelineGraphValue;
use superi_timeline::model::{
    Clip, ClipSource, EditorialProject, LinkedMediaReference, Timeline, Track, TrackItem,
    TrackSemantics, VideoCompositing, VideoTrackSemantics,
};

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

const PROJECT: ProjectId = ProjectId::from_raw(0xc014_0000);
const ROOT: TimelineId = TimelineId::from_raw(0xc014_0001);
const SECONDARY: TimelineId = TimelineId::from_raw(0xc014_0002);
const MEDIA_A: MediaId = MediaId::from_raw(0xc014_0003);
const MEDIA_B: MediaId = MediaId::from_raw(0xc014_0004);
const TRACK: TrackId = TrackId::from_raw(0xc014_0005);
const CLIP: ClipId = ClipId::from_raw(0xc014_0006);
const GRAPH_A: GraphId = GraphId::from_raw(0xc014_0007);
const GRAPH_B: GraphId = GraphId::from_raw(0xc014_0008);

struct TempDirectory {
    path: PathBuf,
}

impl TempDirectory {
    fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "superi-diagnostics-{label}-{}-{}",
            std::process::id(),
            NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir(&path).unwrap();
        Self { path }
    }

    fn project(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDirectory {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn range(start: i64, duration: u64, timebase: Timebase) -> TimeRange {
    TimeRange::new(
        RationalTime::new(start, timebase),
        Duration::new(duration, timebase).unwrap(),
    )
    .unwrap()
}

fn portable(path: &str) -> ReferencedMediaPath {
    ReferencedMediaPath::project_relative(PortableRelativePath::new(path).unwrap())
}

fn extension(extension: &str, record: &str, payload: &[u8]) -> ProjectExtensionRecord {
    ProjectExtensionRecord::new(
        ComponentId::new(extension).unwrap(),
        ProjectExtensionRecordId::new(record).unwrap(),
        SemanticVersion::new(1, 2, 3),
        ProjectExtensionKind::plugin(),
        VersionIdentifier::new(
            ComponentId::new("example.diagnostics-state").unwrap(),
            SemanticVersion::new(4, 5, 6),
        ),
        Default::default(),
        Default::default(),
        ProjectExtensionLifecycle::Enabled,
        None,
        payload.to_vec(),
    )
    .unwrap()
}

fn project_document(reverse_construction: bool) -> ProjectDocument {
    project_document_with_media(reverse_construction, MEDIA_A, "sha256:camera-a")
}

fn project_document_with_media(
    reverse_construction: bool,
    primary_media_id: MediaId,
    primary_fingerprint: &str,
) -> ProjectDocument {
    let rate = FrameRate::FPS_24.timebase();
    let source_range = range(0, 96, rate);
    let clip_source_range = range(0, 48, rate);
    let record_range = range(0, 48, rate);
    let media_a = LinkedMediaReference::with_fingerprint(
        primary_media_id,
        "camera a",
        portable("Media/camera-a.webm").to_target(),
        Some(source_range),
        primary_fingerprint,
    )
    .unwrap();
    let media_b = LinkedMediaReference::with_fingerprint(
        MEDIA_B,
        "camera b",
        portable("Media/camera-b.webm").to_target(),
        Some(source_range),
        "sha256:camera-b",
    )
    .unwrap();
    let clip = Clip::new(
        CLIP,
        "diagnostics clip",
        ClipSource::Media(primary_media_id),
        clip_source_range,
        record_range,
    )
    .unwrap();
    let root = Timeline::new(
        ROOT,
        "diagnostics root",
        rate,
        RationalTime::zero(rate),
        vec![Track::new(
            TRACK,
            "V1",
            TrackSemantics::Video(VideoTrackSemantics::new(
                FrameRate::FPS_24,
                VideoCompositing::Over,
            )),
            vec![TrackItem::Clip(clip)],
        )],
    );
    let secondary = Timeline::new(
        SECONDARY,
        "diagnostics secondary",
        rate,
        RationalTime::zero(rate),
        Vec::new(),
    );
    let mut media = vec![media_a, media_b];
    let mut timelines = vec![root, secondary];
    if reverse_construction {
        media.reverse();
        timelines.reverse();
    }
    let editorial =
        EditorialProject::new(PROJECT, "diagnostics project", media, timelines).unwrap();
    let mut document = ProjectDocument::new(editorial, ROOT).unwrap();

    let mut standalone = vec![
        ProjectGraph::Standalone(
            StandaloneProjectGraph::new(
                "analysis a",
                EditableGraph::<CompiledTimelineGraphValue>::new(GRAPH_A),
            )
            .unwrap(),
        ),
        ProjectGraph::Standalone(
            StandaloneProjectGraph::new(
                "analysis b",
                EditableGraph::<CompiledTimelineGraphValue>::new(GRAPH_B),
            )
            .unwrap(),
        ),
    ];
    if reverse_construction {
        standalone.reverse();
    }
    document
        .edit(0, |draft| {
            for graph in standalone {
                draft.insert_graph(graph)?;
            }
            let stereo = ChannelLayout::stereo();
            let controls = ClipMixControls::new(
                stereo.clone(),
                stereo,
                [ChannelMap::new(
                    ChannelPosition::FrontLeft,
                    ChannelPosition::FrontRight,
                    0.5,
                )?],
            )?
            .with_gain(0.75)?
            .with_fades(120, 240)?;
            draft
                .clip_mix_state_mut()
                .apply(0, &[ClipMixMutation::set(CLIP, controls)])?;
            Ok(())
        })
        .unwrap();
    document
        .execute_settings_transaction(
            ProjectSettingsTransaction::new(
                document.revision(),
                vec![ProjectSettingMutation::set(
                    AUDIO_SAMPLE_RATE_KEY,
                    SettingValue::Integer(96_000),
                )
                .unwrap()],
            )
            .unwrap(),
        )
        .unwrap();
    let mut extensions = vec![
        extension("example.diagnostics-a", "state-a", b"payload-a"),
        extension("example.diagnostics-b", "state-b", b"payload-b"),
    ];
    if reverse_construction {
        extensions.reverse();
    }
    for record in extensions {
        document
            .execute_extension_command(document.revision(), ProjectExtensionCommand::upsert(record))
            .unwrap();
    }
    document
}

fn diagnostics(document: &ProjectDocument) -> ProjectDiagnostics {
    ProjectDiagnostics::from_snapshot(&document.snapshot()).unwrap()
}

fn differing_component_codes(
    left: &ProjectDiagnostics,
    right: &ProjectDiagnostics,
) -> Vec<&'static str> {
    assert_eq!(left.components().len(), right.components().len());
    left.components()
        .iter()
        .zip(right.components())
        .filter(|&(left, right)| left != right)
        .map(|(left, _)| left.code())
        .collect()
}

#[test]
fn semantic_hash_is_versioned_order_independent_and_component_visible() {
    assert_eq!(PROJECT_HASH_ALGORITHM, "sha256");
    assert_eq!(PROJECT_HASH_FORMAT_REVISION, 1);
    let forward = diagnostics(&project_document(false));
    let reversed = diagnostics(&project_document(true));

    assert_eq!(forward, reversed);
    let different_media_id = diagnostics(&project_document_with_media(
        false,
        MediaId::from_raw(0xc014_1003),
        "sha256:camera-a",
    ));
    let different_fingerprint = diagnostics(&project_document_with_media(
        false,
        MEDIA_A,
        "sha256:camera-a-replacement",
    ));
    assert_ne!(forward.content_hash(), different_media_id.content_hash());
    assert_ne!(forward.content_hash(), different_fingerprint.content_hash());
    assert_eq!(forward.project_id(), PROJECT);
    assert_eq!(forward.root_timeline_id(), ROOT);
    assert_eq!(forward.hash_algorithm(), PROJECT_HASH_ALGORITHM);
    assert_eq!(forward.hash_format_revision(), PROJECT_HASH_FORMAT_REVISION);
    assert_eq!(forward.content_hash().to_string().len(), 64);
    assert!(forward
        .content_hash()
        .to_string()
        .bytes()
        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)));

    let components = forward.components();
    assert_eq!(components.len(), 8);
    assert!(matches!(
        components[0],
        ProjectDiagnosticComponent::Timeline { .. }
    ));
    assert!(matches!(
        components[1],
        ProjectDiagnosticComponent::Settings { .. }
    ));
    assert!(matches!(
        components[2],
        ProjectDiagnosticComponent::ClipMix { .. }
    ));
    assert!(matches!(
        components[3],
        ProjectDiagnosticComponent::Extension { .. }
    ));
    assert!(matches!(
        components[4],
        ProjectDiagnosticComponent::Extension { .. }
    ));
    assert!(components[5..]
        .iter()
        .all(|component| matches!(component, ProjectDiagnosticComponent::Graph { .. })));
}

#[test]
fn every_authored_component_changes_the_hash_and_restore_recovers_it() {
    let document = project_document(false);
    let baseline_snapshot = document.snapshot();
    let baseline = diagnostics(&document);

    let mut media = document.clone();
    media
        .execute_media_command(
            media.revision(),
            ProjectMediaCommand::set_path(MEDIA_A, portable("Media/relinked-camera-a.webm")),
        )
        .unwrap();
    let media_diagnostics = diagnostics(&media);
    assert_ne!(media_diagnostics.content_hash(), baseline.content_hash());
    assert_eq!(
        differing_component_codes(&baseline, &media_diagnostics),
        ["timeline"]
    );

    let mut relink_conflict = document.clone();
    relink_conflict
        .execute_media_command(
            relink_conflict.revision(),
            ProjectMediaCommand::consider_relink(
                MEDIA_A,
                portable("Recovered/camera-a.webm"),
                "sha256:wrong-camera",
            ),
        )
        .unwrap();
    let relink_diagnostics = diagnostics(&relink_conflict);
    assert_ne!(relink_diagnostics.content_hash(), baseline.content_hash());
    assert_eq!(
        differing_component_codes(&baseline, &relink_diagnostics),
        ["timeline"]
    );

    let mut settings = document.clone();
    settings
        .execute_settings_transaction(
            ProjectSettingsTransaction::new(
                settings.revision(),
                vec![ProjectSettingMutation::set(
                    AUDIO_SAMPLE_RATE_KEY,
                    SettingValue::Integer(48_000),
                )
                .unwrap()],
            )
            .unwrap(),
        )
        .unwrap();
    let settings_diagnostics = diagnostics(&settings);
    assert_ne!(settings_diagnostics.content_hash(), baseline.content_hash());
    assert_eq!(
        differing_component_codes(&baseline, &settings_diagnostics),
        ["settings"]
    );

    let mut audio = document.clone();
    let mix_revision = audio.clip_mix_state().revision();
    audio
        .edit(audio.revision(), |draft| {
            let stereo = ChannelLayout::stereo();
            let controls = ClipMixControls::new(
                stereo.clone(),
                stereo,
                [ChannelMap::new(
                    ChannelPosition::FrontLeft,
                    ChannelPosition::FrontLeft,
                    1.0,
                )?],
            )?
            .with_gain(0.25)?;
            draft
                .clip_mix_state_mut()
                .apply(mix_revision, &[ClipMixMutation::set(CLIP, controls)])?;
            Ok(())
        })
        .unwrap();
    let audio_diagnostics = diagnostics(&audio);
    assert_ne!(audio_diagnostics.content_hash(), baseline.content_hash());
    assert_eq!(
        differing_component_codes(&baseline, &audio_diagnostics),
        ["clip_mix"]
    );

    let mut extension_state = document.clone();
    extension_state
        .execute_extension_command(
            extension_state.revision(),
            ProjectExtensionCommand::upsert(extension(
                "example.diagnostics-a",
                "state-a",
                b"replacement-payload",
            )),
        )
        .unwrap();
    let extension_diagnostics = diagnostics(&extension_state);
    assert_ne!(
        extension_diagnostics.content_hash(),
        baseline.content_hash()
    );
    assert_eq!(
        differing_component_codes(&baseline, &extension_diagnostics),
        ["extension"]
    );

    let mut graph = document.clone();
    graph
        .edit(graph.revision(), |draft| {
            match draft.graph_mut(GRAPH_A)? {
                ProjectGraph::Standalone(graph) => graph.set_name("renamed analysis")?,
                _ => panic!("expected standalone graph"),
            }
            Ok(())
        })
        .unwrap();
    let graph_diagnostics = diagnostics(&graph);
    assert_ne!(graph_diagnostics.content_hash(), baseline.content_hash());
    assert_eq!(
        differing_component_codes(&baseline, &graph_diagnostics),
        ["graph"]
    );

    let mutated = diagnostics(&media);
    media
        .restore_snapshot(media.revision(), &baseline_snapshot)
        .unwrap();
    let restored = diagnostics(&media);
    assert_ne!(mutated.content_hash(), restored.content_hash());
    assert_eq!(restored.content_hash(), baseline.content_hash());
    assert!(restored.observed_document_revision() > baseline.observed_document_revision());
}

#[test]
fn persistence_path_and_sqlite_layout_do_not_change_semantic_hash() {
    let directory = TempDirectory::new("persistence-layout");
    let first_path = directory.project("first.superi");
    let second_path = directory.project("second.superi");
    let document = project_document(false);
    let expected = diagnostics(&document);

    let mut first = ProjectDatabase::create(&first_path).unwrap();
    first.replace(&document.snapshot()).unwrap();
    let mut second = ProjectDatabase::create(&second_path).unwrap();
    second.replace(&document.snapshot()).unwrap();
    drop(first);
    drop(second);

    Connection::open(&second_path)
        .unwrap()
        .execute_batch("PRAGMA page_size = 8192; VACUUM;")
        .unwrap();
    assert_ne!(
        std::fs::read(&first_path).unwrap(),
        std::fs::read(&second_path).unwrap()
    );

    let first = ProjectDatabase::open_read_only(&first_path)
        .unwrap()
        .load()
        .unwrap();
    let second = ProjectDatabase::open_read_only(&second_path)
        .unwrap()
        .load()
        .unwrap();
    assert_eq!(diagnostics(&first), expected);
    assert_eq!(diagnostics(&second), expected);
}
