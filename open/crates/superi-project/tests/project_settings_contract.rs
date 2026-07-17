use superi_core::error::ErrorCategory;
use superi_core::ids::{GeneratorId, ProjectId, TimelineId, TrackId};
use superi_core::settings::SettingValue;
use superi_core::time::{Duration, FrameRate, RationalTime, TimeRange, Timebase};
use superi_project::document::ProjectDocument;
use superi_project::settings::{
    ProjectSettingMutation, ProjectSettingsTransaction, AUDIO_OUTPUT_LAYOUT_KEY,
    AUDIO_SAMPLE_RATE_KEY, CACHE_MAX_BYTES_KEY, CACHE_MAX_FRAMES_KEY, CACHE_MODE_KEY,
    COLOR_CONFIG_FINGERPRINT_KEY, COLOR_CONFIG_ID_KEY, COLOR_MODE_KEY, COLOR_WORKING_SPACE_KEY,
    MAX_PROJECT_SETTING_MUTATIONS, PROXY_MODE_KEY, PROXY_QUALITY_KEY, RENDER_ALPHA_MODE_KEY,
    RENDER_COLOR_TARGET_KEY, RENDER_EXTENT_MODE_KEY, RENDER_FRAME_RATE_MODE_KEY, RENDER_HEIGHT_KEY,
    RENDER_PIXEL_FORMAT_KEY, RENDER_RATE_DENOMINATOR_KEY, RENDER_RATE_NUMERATOR_KEY,
    RENDER_WIDTH_KEY, TIMELINE_RATE_DENOMINATOR_KEY, TIMELINE_RATE_NUMERATOR_KEY,
    TIMELINE_TIMECODE_MODE_KEY,
};
use superi_project::ProjectDatabase;
use superi_timeline::model::{
    EditorialProject, Generator, LinkedMediaReference, Timeline, Track, TrackItem, TrackSemantics,
    VideoCompositing, VideoTrackSemantics,
};

const PROJECT: ProjectId = ProjectId::from_raw(9_001);
const ROOT: TimelineId = TimelineId::from_raw(9_002);
const TRACK: TrackId = TrackId::from_raw(9_003);
const GENERATOR: GeneratorId = GeneratorId::from_raw(9_004);

fn project_document() -> ProjectDocument {
    let rate = Timebase::integer(24).unwrap();
    let range = TimeRange::new(RationalTime::zero(rate), Duration::new(48, rate).unwrap()).unwrap();
    let timeline = Timeline::new(
        ROOT,
        "settings timeline",
        rate,
        RationalTime::zero(rate),
        vec![Track::new(
            TRACK,
            "V1",
            TrackSemantics::Video(VideoTrackSemantics::new(
                FrameRate::FPS_24,
                VideoCompositing::Over,
            )),
            vec![TrackItem::Generator(Generator::new(
                GENERATOR,
                "settings generator",
                "solid_color",
                Default::default(),
                range,
            ))],
        )],
    );
    let editorial = EditorialProject::new(
        PROJECT,
        "settings project",
        std::iter::empty::<LinkedMediaReference>(),
        [timeline],
    )
    .unwrap();
    ProjectDocument::new(editorial, ROOT).unwrap()
}

#[test]
fn settings_transactions_are_strict_revision_fenced_atomic_and_noop_aware() {
    let mut document = project_document();
    assert_eq!(
        document.settings().integer(TIMELINE_RATE_NUMERATOR_KEY),
        Some(24)
    );
    assert_eq!(
        document.settings().integer(TIMELINE_RATE_DENOMINATOR_KEY),
        Some(1)
    );
    assert_eq!(
        document.settings().integer(AUDIO_SAMPLE_RATE_KEY),
        Some(48_000)
    );
    assert_eq!(
        document.settings().text(AUDIO_OUTPUT_LAYOUT_KEY),
        Some("stereo")
    );

    let before = document.snapshot();
    let invalid = ProjectSettingsTransaction::new(
        0,
        vec![
            ProjectSettingMutation::set(CACHE_MODE_KEY, SettingValue::Text("bounded".into()))
                .unwrap(),
            ProjectSettingMutation::set(CACHE_MAX_BYTES_KEY, SettingValue::Integer(0)).unwrap(),
            ProjectSettingMutation::set(CACHE_MAX_FRAMES_KEY, SettingValue::Integer(32)).unwrap(),
        ],
    )
    .unwrap();
    let error = document.execute_settings_transaction(invalid).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(document.snapshot(), before);

    let committed = ProjectSettingsTransaction::new(
        0,
        vec![
            ProjectSettingMutation::set(AUDIO_SAMPLE_RATE_KEY, SettingValue::Integer(96_000))
                .unwrap(),
            ProjectSettingMutation::set(
                AUDIO_OUTPUT_LAYOUT_KEY,
                SettingValue::Text("surround_5_1".into()),
            )
            .unwrap(),
        ],
    )
    .unwrap();
    let snapshot = document.execute_settings_transaction(committed).unwrap();
    assert_eq!(snapshot.revision(), 1);
    assert_eq!(
        snapshot.settings().integer(AUDIO_SAMPLE_RATE_KEY),
        Some(96_000)
    );
    assert_eq!(
        snapshot.settings().text(AUDIO_OUTPUT_LAYOUT_KEY),
        Some("surround_5_1")
    );
    assert_eq!(snapshot.editorial_project(), before.editorial_project());

    let noop = ProjectSettingsTransaction::new(
        1,
        vec![
            ProjectSettingMutation::set(AUDIO_SAMPLE_RATE_KEY, SettingValue::Integer(96_000))
                .unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(
        document
            .execute_settings_transaction(noop)
            .unwrap()
            .revision(),
        1
    );

    let stale = ProjectSettingsTransaction::new(
        0,
        vec![
            ProjectSettingMutation::set(AUDIO_SAMPLE_RATE_KEY, SettingValue::Integer(44_100))
                .unwrap(),
        ],
    )
    .unwrap();
    assert_eq!(
        document
            .execute_settings_transaction(stale)
            .unwrap_err()
            .category(),
        ErrorCategory::Conflict
    );
}

#[test]
fn settings_are_a_durable_manifest_component() {
    let mut document = project_document();
    let transaction = ProjectSettingsTransaction::new(
        0,
        vec![
            ProjectSettingMutation::set(CACHE_MODE_KEY, SettingValue::Text("bounded".into()))
                .unwrap(),
            ProjectSettingMutation::set(
                CACHE_MAX_BYTES_KEY,
                SettingValue::Integer(512 * 1024 * 1024),
            )
            .unwrap(),
            ProjectSettingMutation::set(CACHE_MAX_FRAMES_KEY, SettingValue::Integer(128)).unwrap(),
        ],
    )
    .unwrap();
    document.execute_settings_transaction(transaction).unwrap();

    let mut database = ProjectDatabase::memory().unwrap();
    database.replace(&document.snapshot()).unwrap();
    let loaded = database.load().unwrap();

    assert_eq!(loaded.snapshot(), document.snapshot());
    assert_eq!(loaded.settings().text(CACHE_MODE_KEY), Some("bounded"));
    assert_eq!(
        loaded.settings().integer(CACHE_MAX_BYTES_KEY),
        Some(512 * 1024 * 1024)
    );
    assert_eq!(loaded.settings().integer(CACHE_MAX_FRAMES_KEY), Some(128));
}

#[test]
fn every_policy_domain_and_conditional_key_is_validated_as_one_candidate() {
    let mut document = project_document();
    let configured = ProjectSettingsTransaction::new(
        0,
        vec![
            ProjectSettingMutation::set(TIMELINE_RATE_NUMERATOR_KEY, SettingValue::Integer(30_000))
                .unwrap(),
            ProjectSettingMutation::set(
                TIMELINE_RATE_DENOMINATOR_KEY,
                SettingValue::Integer(1_001),
            )
            .unwrap(),
            ProjectSettingMutation::set(
                TIMELINE_TIMECODE_MODE_KEY,
                SettingValue::Text("drop_frame".into()),
            )
            .unwrap(),
            ProjectSettingMutation::set(COLOR_MODE_KEY, SettingValue::Text("pinned_config".into()))
                .unwrap(),
            ProjectSettingMutation::set(
                COLOR_WORKING_SPACE_KEY,
                SettingValue::Text("logc3".into()),
            )
            .unwrap(),
            ProjectSettingMutation::set(
                COLOR_CONFIG_ID_KEY,
                SettingValue::Text("studio.config".into()),
            )
            .unwrap(),
            ProjectSettingMutation::set(
                COLOR_CONFIG_FINGERPRINT_KEY,
                SettingValue::Text("b".repeat(64)),
            )
            .unwrap(),
            ProjectSettingMutation::set(AUDIO_SAMPLE_RATE_KEY, SettingValue::Integer(96_000))
                .unwrap(),
            ProjectSettingMutation::set(
                AUDIO_OUTPUT_LAYOUT_KEY,
                SettingValue::Text("surround_7_1".into()),
            )
            .unwrap(),
            ProjectSettingMutation::set(CACHE_MODE_KEY, SettingValue::Text("bounded".into()))
                .unwrap(),
            ProjectSettingMutation::set(CACHE_MAX_BYTES_KEY, SettingValue::Integer(1_024)).unwrap(),
            ProjectSettingMutation::set(CACHE_MAX_FRAMES_KEY, SettingValue::Integer(8)).unwrap(),
            ProjectSettingMutation::set(PROXY_MODE_KEY, SettingValue::Text("prefer".into()))
                .unwrap(),
            ProjectSettingMutation::set(PROXY_QUALITY_KEY, SettingValue::Text("quarter".into()))
                .unwrap(),
            ProjectSettingMutation::set(
                RENDER_FRAME_RATE_MODE_KEY,
                SettingValue::Text("explicit".into()),
            )
            .unwrap(),
            ProjectSettingMutation::set(RENDER_RATE_NUMERATOR_KEY, SettingValue::Integer(60))
                .unwrap(),
            ProjectSettingMutation::set(RENDER_RATE_DENOMINATOR_KEY, SettingValue::Integer(1))
                .unwrap(),
            ProjectSettingMutation::set(
                RENDER_EXTENT_MODE_KEY,
                SettingValue::Text("explicit".into()),
            )
            .unwrap(),
            ProjectSettingMutation::set(RENDER_WIDTH_KEY, SettingValue::Integer(3_840)).unwrap(),
            ProjectSettingMutation::set(RENDER_HEIGHT_KEY, SettingValue::Integer(2_160)).unwrap(),
            ProjectSettingMutation::set(
                RENDER_PIXEL_FORMAT_KEY,
                SettingValue::Text("rgba32_float".into()),
            )
            .unwrap(),
            ProjectSettingMutation::set(
                RENDER_ALPHA_MODE_KEY,
                SettingValue::Text("straight".into()),
            )
            .unwrap(),
            ProjectSettingMutation::set(
                RENDER_COLOR_TARGET_KEY,
                SettingValue::Text("delivery".into()),
            )
            .unwrap(),
        ],
    )
    .unwrap();
    document.execute_settings_transaction(configured).unwrap();

    let inherit = ProjectSettingsTransaction::new(
        1,
        vec![
            ProjectSettingMutation::set(
                COLOR_MODE_KEY,
                SettingValue::Text("built_in_acescg".into()),
            )
            .unwrap(),
            ProjectSettingMutation::set(
                COLOR_WORKING_SPACE_KEY,
                SettingValue::Text("acescg".into()),
            )
            .unwrap(),
            ProjectSettingMutation::remove(COLOR_CONFIG_ID_KEY).unwrap(),
            ProjectSettingMutation::remove(COLOR_CONFIG_FINGERPRINT_KEY).unwrap(),
            ProjectSettingMutation::set(CACHE_MODE_KEY, SettingValue::Text("automatic".into()))
                .unwrap(),
            ProjectSettingMutation::remove(CACHE_MAX_BYTES_KEY).unwrap(),
            ProjectSettingMutation::remove(CACHE_MAX_FRAMES_KEY).unwrap(),
            ProjectSettingMutation::set(
                RENDER_FRAME_RATE_MODE_KEY,
                SettingValue::Text("timeline".into()),
            )
            .unwrap(),
            ProjectSettingMutation::remove(RENDER_RATE_NUMERATOR_KEY).unwrap(),
            ProjectSettingMutation::remove(RENDER_RATE_DENOMINATOR_KEY).unwrap(),
            ProjectSettingMutation::set(
                RENDER_EXTENT_MODE_KEY,
                SettingValue::Text("source".into()),
            )
            .unwrap(),
            ProjectSettingMutation::remove(RENDER_WIDTH_KEY).unwrap(),
            ProjectSettingMutation::remove(RENDER_HEIGHT_KEY).unwrap(),
        ],
    )
    .unwrap();
    let snapshot = document.execute_settings_transaction(inherit).unwrap();
    assert_eq!(snapshot.revision(), 2);
    assert!(snapshot.settings().value(COLOR_CONFIG_ID_KEY).is_none());
    assert!(snapshot.settings().value(CACHE_MAX_BYTES_KEY).is_none());
    assert!(snapshot.settings().value(RENDER_WIDTH_KEY).is_none());

    let duplicate = ProjectSettingsTransaction::new(
        2,
        vec![
            ProjectSettingMutation::set(AUDIO_SAMPLE_RATE_KEY, SettingValue::Integer(48_000))
                .unwrap(),
            ProjectSettingMutation::set(AUDIO_SAMPLE_RATE_KEY, SettingValue::Integer(96_000))
                .unwrap(),
        ],
    )
    .unwrap_err();
    assert_eq!(duplicate.category(), ErrorCategory::InvalidInput);
    assert_eq!(
        ProjectSettingsTransaction::new(2, vec![])
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    let oversized = (0..=MAX_PROJECT_SETTING_MUTATIONS)
        .map(|index| {
            ProjectSettingMutation::set(
                format!("superi.project.test.key{index}"),
                SettingValue::Integer(1),
            )
            .unwrap()
        })
        .collect();
    assert_eq!(
        ProjectSettingsTransaction::new(2, oversized)
            .unwrap_err()
            .category(),
        ErrorCategory::ResourceExhausted
    );

    for mutation in [
        ProjectSettingMutation::set(
            TIMELINE_TIMECODE_MODE_KEY,
            SettingValue::Text("drop_frame".into()),
        )
        .unwrap(),
        ProjectSettingMutation::set(AUDIO_SAMPLE_RATE_KEY, SettingValue::Boolean(true)).unwrap(),
        ProjectSettingMutation::set("superi.project.unknown.setting", SettingValue::Integer(1))
            .unwrap(),
        ProjectSettingMutation::remove(AUDIO_OUTPUT_LAYOUT_KEY).unwrap(),
    ] {
        let mut invalid_document = project_document();
        let before = invalid_document.snapshot();
        let transaction = ProjectSettingsTransaction::new(0, vec![mutation]).unwrap();
        assert!(invalid_document
            .execute_settings_transaction(transaction)
            .is_err());
        assert_eq!(invalid_document.snapshot(), before);
    }
}
