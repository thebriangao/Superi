use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use superi_core::color_space::ColorSpace;
use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::geometry::PixelBounds;
use superi_core::ids::MediaId;
use superi_core::pixel::AlphaMode;
use superi_image::channels::{ChannelIndex, ChannelList};
use superi_image::io::{ReadOptions, StillImage, StillImageLayer, WriteOptions};
use superi_image::model::{
    ByteAlignment, ChannelSlice, ChannelStorageLayout, ImageStorage, StoragePlane,
};
use superi_image::sequence::{
    ImageSequenceManifest, ImageSequencePattern, ImageSequenceReader, ImageSequenceWriter,
    MissingFramePolicy, SequenceSubstitution,
};
use superi_image::tiling::{
    ImageAccess, ImageAccessDescriptor, ImageOrganization, ImageSequencePosition, ImageTile,
    LevelRoundingMode, MipLevel, MipMode, TileDescription, TileIndex,
};
use superi_image::value::ImageSampleType;

static NEXT_DIRECTORY: AtomicU64 = AtomicU64::new(0);

struct TemporaryDirectory(PathBuf);

impl TemporaryDirectory {
    fn new(test_name: &str) -> Self {
        let serial = NEXT_DIRECTORY.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "superi-image-sequence-{}-{test_name}-{serial}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir(&path).unwrap();
        Self(path)
    }

    fn path(&self) -> &Path {
        &self.0
    }

    fn touch(&self, name: &str) -> PathBuf {
        let path = self.0.join(name);
        fs::write(&path, name.as_bytes()).unwrap();
        path
    }
}

impl Drop for TemporaryDirectory {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn rgba16_image(samples: [u16; 8]) -> StillImage {
    let bounds = PixelBounds::from_origin_size(0, 0, 2, 1).unwrap();
    rgba16_image_with_bounds(bounds, samples)
}

fn rgba16_image_with_bounds(bounds: PixelBounds, samples: [u16; 8]) -> StillImage {
    let descriptor = ImageAccessDescriptor::new(
        bounds,
        bounds,
        ChannelList::from_full_names(["R", "G", "B", "A"]).unwrap(),
        vec![ImageSampleType::U16; 4],
        ColorSpace::SRGB,
        AlphaMode::Straight,
    )
    .unwrap();
    let bytes = samples
        .into_iter()
        .flat_map(u16::to_ne_bytes)
        .collect::<Vec<_>>();
    let storage = ImageStorage::new(
        bounds,
        ChannelStorageLayout::Interleaved,
        vec![StoragePlane::new(Arc::from(bytes), 0, 16, ByteAlignment::new(2).unwrap()).unwrap()],
        (0..4)
            .map(|channel| ChannelSlice::new(0, channel * 2, 2, 8).unwrap())
            .collect(),
    )
    .unwrap();
    StillImage::from_access(ImageAccess::from_scanline(descriptor, storage).unwrap())
}

fn f16_alpha_red_tile(bounds: PixelBounds, alpha: u16, red: u16) -> ImageStorage {
    let sample_count = bounds.width() as usize * bounds.height() as usize;
    let plane = |value: u16| {
        (0..sample_count)
            .flat_map(|_| value.to_ne_bytes())
            .collect::<Vec<_>>()
    };
    ImageStorage::new(
        bounds,
        ChannelStorageLayout::Planar,
        vec![
            StoragePlane::new(
                Arc::from(plane(alpha)),
                0,
                bounds.width() as usize * 2,
                ByteAlignment::new(2).unwrap(),
            )
            .unwrap(),
            StoragePlane::new(
                Arc::from(plane(red)),
                0,
                bounds.width() as usize * 2,
                ByteAlignment::new(2).unwrap(),
            )
            .unwrap(),
        ],
        vec![
            ChannelSlice::new(0, 0, 2, 2).unwrap(),
            ChannelSlice::new(1, 0, 2, 2).unwrap(),
        ],
    )
    .unwrap()
}

fn multipart_tiled_mip_exr_image() -> StillImage {
    let display = PixelBounds::new(-8, -4, 8, 12).unwrap();
    let beauty_bounds = PixelBounds::from_origin_size(-2, 3, 3, 3).unwrap();
    let beauty_descriptor = ImageAccessDescriptor::new(
        beauty_bounds,
        display,
        ChannelList::from_full_names(["A", "R"]).unwrap(),
        vec![ImageSampleType::F16; 2],
        ColorSpace::UNSPECIFIED,
        AlphaMode::Premultiplied,
    )
    .unwrap();
    let beauty_tiles = vec![
        ImageTile::new(
            MipLevel::BASE,
            TileIndex::new(0, 0),
            f16_alpha_red_tile(PixelBounds::new(-2, 3, 0, 5).unwrap(), 0x3800, 0x3c00),
        ),
        ImageTile::new(
            MipLevel::BASE,
            TileIndex::new(1, 0),
            f16_alpha_red_tile(PixelBounds::new(0, 3, 1, 5).unwrap(), 0x3800, 0x3c00),
        ),
        ImageTile::new(
            MipLevel::BASE,
            TileIndex::new(0, 1),
            f16_alpha_red_tile(PixelBounds::new(-2, 5, 0, 6).unwrap(), 0x3800, 0x3c00),
        ),
        ImageTile::new(
            MipLevel::BASE,
            TileIndex::new(1, 1),
            f16_alpha_red_tile(PixelBounds::new(0, 5, 1, 6).unwrap(), 0x3800, 0x3c00),
        ),
        ImageTile::new(
            MipLevel::new(1),
            TileIndex::new(0, 0),
            f16_alpha_red_tile(PixelBounds::new(-2, 3, -1, 4).unwrap(), 0x3800, 0x3c00),
        ),
    ];
    let beauty = ImageAccess::tiled(
        beauty_descriptor,
        TileDescription::new(2, 2, MipMode::Mipmap, LevelRoundingMode::Down).unwrap(),
        beauty_tiles,
    )
    .unwrap();

    let depth_bounds = PixelBounds::from_origin_size(4, -1, 1, 2).unwrap();
    let depth_descriptor = ImageAccessDescriptor::new(
        depth_bounds,
        display,
        ChannelList::from_full_names(["Z"]).unwrap(),
        vec![ImageSampleType::F32],
        ColorSpace::UNSPECIFIED,
        AlphaMode::Opaque,
    )
    .unwrap();
    let depth = ImageAccess::from_scanline(
        depth_descriptor,
        ImageStorage::new(
            depth_bounds,
            ChannelStorageLayout::Planar,
            vec![StoragePlane::new(
                Arc::from(
                    [5.0_f32, 9.0]
                        .into_iter()
                        .flat_map(|value| value.to_ne_bytes())
                        .collect::<Vec<_>>(),
                ),
                0,
                4,
                ByteAlignment::new(4).unwrap(),
            )
            .unwrap()],
            vec![ChannelSlice::new(0, 0, 4, 4).unwrap()],
        )
        .unwrap(),
    )
    .unwrap();
    StillImage::new(vec![
        StillImageLayer::new(Some("beauty".to_owned()), beauty).unwrap(),
        StillImageLayer::new(Some("depth".to_owned()), depth).unwrap(),
    ])
    .unwrap()
}

#[test]
fn pattern_parsing_preserves_signed_numbering_padding_and_surrounding_name() {
    let directory = TemporaryDirectory::new("parse");
    let selected = directory.path().join("shot.v2.beauty.-0001.left.exr");

    let parsed = ImageSequencePattern::parse_frame_path(&selected).unwrap();
    assert_eq!(parsed.frame_number(), -1);
    assert_eq!(parsed.pattern().directory(), directory.path());
    assert_eq!(parsed.pattern().prefix(), "shot.v2.beauty.");
    assert_eq!(parsed.pattern().suffix(), ".left.exr");
    assert_eq!(parsed.pattern().zero_padding(), 4);
    assert_eq!(
        parsed.pattern().path_for_frame(-12).unwrap(),
        directory.path().join("shot.v2.beauty.-0012.left.exr")
    );
    assert_eq!(
        parsed.pattern().path_for_frame(10_000).unwrap(),
        directory.path().join("shot.v2.beauty.10000.left.exr")
    );
}

#[test]
fn malformed_or_unnumbered_paths_fail_with_actionable_shared_errors() {
    for path in [
        Path::new("plate.exr"),
        Path::new(""),
        Path::new("plate.999999999999999999999.exr"),
    ] {
        let error = ImageSequencePattern::parse_frame_path(path).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::InvalidInput);
        assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
        assert!(!error.contexts().is_empty());
    }

    assert_eq!(
        ImageSequencePattern::new(PathBuf::from("plates"), "plate.", ".exr", 0)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
}

#[test]
fn discovery_is_deterministic_and_keeps_explicit_logical_and_file_numbering() {
    let directory = TemporaryDirectory::new("discover");
    directory.touch("plate.1005.exr");
    let selected = directory.touch("plate.1001.exr");
    directory.touch("plate.1003.exr");
    directory.touch("plate.01002.exr");
    directory.touch("plate.1003.png");
    directory.touch("other.1003.exr");
    fs::create_dir(directory.path().join("plate.1007.exr")).unwrap();

    let manifest = ImageSequenceManifest::discover(&selected, 2).unwrap();
    assert_eq!(manifest.first_frame_number(), 1001);
    assert_eq!(manifest.last_frame_number(), 1005);
    assert_eq!(manifest.frame_step(), 2);
    assert_eq!(manifest.logical_frame_count(), 3);
    assert_eq!(manifest.available_frame_count(), 3);
    assert!(manifest.missing_frame_numbers().is_empty());
    assert_eq!(
        manifest
            .frames()
            .map(|frame| (
                frame.image_number(),
                frame.file_frame_number(),
                frame.path().map(Path::to_path_buf),
            ))
            .collect::<Vec<_>>(),
        vec![
            (0, 1001, Some(directory.path().join("plate.1001.exr"))),
            (1, 1003, Some(directory.path().join("plate.1003.exr"))),
            (2, 1005, Some(directory.path().join("plate.1005.exr"))),
        ]
    );

    let rediscovered = ImageSequenceManifest::discover(&selected, 2).unwrap();
    assert_eq!(manifest, rediscovered);
}

#[test]
fn discovery_reports_gaps_instead_of_inferring_a_larger_frame_step() {
    let directory = TemporaryDirectory::new("gaps");
    let selected = directory.touch("render.0001.exr");
    directory.touch("render.0003.exr");
    directory.touch("render.0004.exr");

    let manifest = ImageSequenceManifest::discover(&selected, 1).unwrap();
    assert_eq!(manifest.logical_frame_count(), 4);
    assert_eq!(manifest.available_frame_count(), 3);
    assert_eq!(manifest.missing_frame_numbers(), &[2]);
    assert_eq!(manifest.frame(1).unwrap().file_frame_number(), 2);
    assert!(manifest.frame(1).unwrap().path().is_none());

    let error = ImageSequenceManifest::discover(&selected, 2).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert!(error
        .contexts()
        .iter()
        .any(|context| context.field("file_frame") == Some("4")));
}

#[test]
fn missing_frame_policy_distinguishes_error_hold_and_black_resolution() {
    let directory = TemporaryDirectory::new("missing-policy");
    let selected = directory.touch("comp.1001.exr");
    directory.touch("comp.1003.exr");
    directory.touch("comp.1004.exr");
    let manifest = ImageSequenceManifest::discover(&selected, 1).unwrap();

    let error = manifest.resolve(1, MissingFramePolicy::Error).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::NotFound);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert!(error
        .contexts()
        .iter()
        .any(|context| context.field("file_frame") == Some("1002")));

    let held = manifest.resolve(1, MissingFramePolicy::Hold).unwrap();
    assert_eq!(held.requested().image_number(), 1);
    assert_eq!(held.requested().file_frame_number(), 1002);
    assert_eq!(held.source_frame_number(), Some(1001));
    assert_eq!(held.substitution(), SequenceSubstitution::Hold);
    assert_eq!(
        held.read_path(),
        Some(directory.path().join("comp.1001.exr").as_path())
    );

    let black = manifest.resolve(1, MissingFramePolicy::Black).unwrap();
    assert_eq!(black.requested().file_frame_number(), 1002);
    assert_eq!(black.source_frame_number(), None);
    assert_eq!(black.substitution(), SequenceSubstitution::Black);
    assert_eq!(
        black.reference_path(),
        Some(directory.path().join("comp.1001.exr").as_path())
    );

    let exact = manifest.resolve(2, MissingFramePolicy::Error).unwrap();
    assert_eq!(exact.requested().file_frame_number(), 1003);
    assert_eq!(exact.source_frame_number(), Some(1003));
    assert_eq!(exact.substitution(), SequenceSubstitution::None);
    assert_eq!(exact.reference_path(), None);
}

#[test]
fn leading_missing_frames_cannot_hold_but_can_reference_the_next_frame_for_black() {
    let directory = TemporaryDirectory::new("leading-gap");
    let selected = directory.touch("comp.1001.exr");
    directory.touch("comp.1003.exr");
    let pattern = ImageSequencePattern::parse_frame_path(&selected)
        .unwrap()
        .into_pattern();
    let manifest = ImageSequenceManifest::discover_range(pattern, 1000, 1003, 1).unwrap();

    let hold_error = manifest.resolve(0, MissingFramePolicy::Hold).unwrap_err();
    assert_eq!(hold_error.category(), ErrorCategory::NotFound);

    let black = manifest.resolve(0, MissingFramePolicy::Black).unwrap();
    assert_eq!(black.requested().file_frame_number(), 1000);
    assert_eq!(black.source_frame_number(), None);
    assert_eq!(
        black.reference_path(),
        Some(directory.path().join("comp.1001.exr").as_path())
    );
}

#[test]
fn discovery_rejects_zero_steps_missing_directories_and_out_of_range_addresses() {
    let directory = TemporaryDirectory::new("invalid-discovery");
    let selected = directory.touch("plate.0001.exr");

    assert_eq!(
        ImageSequenceManifest::discover(&selected, 0)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        ImageSequenceManifest::discover(directory.path().join("missing/plate.0001.exr"), 1)
            .unwrap_err()
            .category(),
        ErrorCategory::NotFound
    );

    let manifest = ImageSequenceManifest::discover(&selected, 1).unwrap();
    assert_eq!(
        manifest.frame(1).unwrap_err().category(),
        ErrorCategory::InvalidInput
    );
    assert_eq!(
        manifest
            .resolve(1, MissingFramePolicy::Error)
            .unwrap_err()
            .category(),
        ErrorCategory::InvalidInput
    );
}

#[test]
fn concrete_sequence_write_discover_and_read_preserve_semantics_and_requested_identity() {
    fn assert_send<T: Send>() {}
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send::<ImageSequenceWriter>();
    assert_send_sync::<ImageSequenceReader>();
    assert_send_sync::<ImageSequenceManifest>();

    let directory = TemporaryDirectory::new("real-io");
    let pattern =
        ImageSequencePattern::new(directory.path().to_path_buf(), "plate.", ".png", 4).unwrap();
    let first_image = rgba16_image([0, 1, 2, u16::MAX, 3, 4, 5, 32_768]);
    let middle_image = rgba16_image([6, 7, 8, u16::MAX, 9, 10, 11, u16::MAX]);
    let last_image = rgba16_image([12, 13, 14, u16::MAX, 15, 16, 17, u16::MAX]);
    let mut writer =
        ImageSequenceWriter::new(pattern.clone(), 1001, 1, WriteOptions::default()).unwrap();

    let first = writer.write_image(&first_image).unwrap();
    assert_eq!(first.image_number(), 0);
    assert_eq!(first.file_frame_number(), 1001);
    assert_eq!(
        first.path(),
        Some(pattern.path_for_frame(1001).unwrap().as_path())
    );
    writer.write_image(&middle_image).unwrap();
    writer.write_image(&last_image).unwrap();
    assert_eq!(writer.frames_written(), 3);

    fs::remove_file(pattern.path_for_frame(1002).unwrap()).unwrap();
    let manifest =
        ImageSequenceManifest::discover(pattern.path_for_frame(1001).unwrap(), 1).unwrap();
    assert_eq!(manifest.missing_frame_numbers(), &[1002]);
    let media_id = MediaId::from_raw(404);

    let exact_reader = ImageSequenceReader::new(
        manifest.clone(),
        media_id,
        MissingFramePolicy::Error,
        ReadOptions::default(),
    );
    let exact = exact_reader.read_image(0).unwrap();
    assert_eq!(exact.substitution(), SequenceSubstitution::None);
    assert_eq!(exact.source_frame_number(), Some(1001));
    let exact_access = exact.image().single_access().unwrap();
    assert_eq!(
        exact_access.descriptor().sequence_position(),
        Some(ImageSequencePosition::new(media_id, 0))
    );
    assert_eq!(
        exact_access.scanline_storage().unwrap(),
        first_image
            .single_access()
            .unwrap()
            .scanline_storage()
            .unwrap()
    );
    assert_eq!(
        exact_access.descriptor().channels(),
        first_image.single_access().unwrap().descriptor().channels()
    );
    assert_eq!(
        exact_access.descriptor().sample_types(),
        first_image
            .single_access()
            .unwrap()
            .descriptor()
            .sample_types()
    );
    assert_eq!(exact_access.descriptor().alpha_mode(), AlphaMode::Straight);

    let hold_reader = ImageSequenceReader::new(
        manifest,
        media_id,
        MissingFramePolicy::Hold,
        ReadOptions::default(),
    );
    let held = hold_reader.read_image(1).unwrap();
    assert_eq!(held.requested().file_frame_number(), 1002);
    assert_eq!(held.source_frame_number(), Some(1001));
    assert_eq!(held.substitution(), SequenceSubstitution::Hold);
    assert_eq!(
        held.image()
            .single_access()
            .unwrap()
            .descriptor()
            .sequence_position(),
        Some(ImageSequencePosition::new(media_id, 1))
    );
    assert_eq!(
        held.image()
            .single_access()
            .unwrap()
            .scanline_storage()
            .unwrap(),
        exact_access.scanline_storage().unwrap()
    );
}

#[test]
fn black_policy_generates_opaque_black_without_changing_image_representation() {
    let directory = TemporaryDirectory::new("black-io");
    let pattern =
        ImageSequencePattern::new(directory.path().to_path_buf(), "comp.", ".png", 4).unwrap();
    let image = rgba16_image([10, 20, 30, 40, 50, 60, 70, 80]);
    let mut writer =
        ImageSequenceWriter::new(pattern.clone(), 1, 1, WriteOptions::default()).unwrap();
    writer.write_image(&image).unwrap();
    writer.write_image(&image).unwrap();
    writer.write_image(&image).unwrap();
    fs::remove_file(pattern.path_for_frame(2).unwrap()).unwrap();

    let manifest = ImageSequenceManifest::discover(pattern.path_for_frame(1).unwrap(), 1).unwrap();
    let reader = ImageSequenceReader::new(
        manifest,
        MediaId::from_raw(505),
        MissingFramePolicy::Black,
        ReadOptions::default(),
    );
    let reference = reader.read_image(0).unwrap();
    let black = reader.read_image(1).unwrap();
    assert_eq!(black.substitution(), SequenceSubstitution::Black);
    assert_eq!(black.source_frame_number(), None);
    let access = black.image().single_access().unwrap();
    assert_eq!(
        access.organization(),
        image.single_access().unwrap().organization()
    );
    assert_eq!(
        access.descriptor().channels(),
        image.single_access().unwrap().descriptor().channels()
    );
    assert_eq!(
        access.descriptor().sample_types(),
        &[ImageSampleType::U16; 4]
    );
    assert_eq!(access.descriptor().alpha_mode(), AlphaMode::Straight);
    assert_eq!(
        access.descriptor().data_window(),
        image.single_access().unwrap().descriptor().data_window()
    );
    assert_eq!(
        access.descriptor().display_window(),
        image.single_access().unwrap().descriptor().display_window()
    );
    assert_eq!(
        access.descriptor().color_tags(),
        reference
            .image()
            .single_access()
            .unwrap()
            .descriptor()
            .color_tags()
    );
    assert_eq!(
        access.descriptor().metadata(),
        reference
            .image()
            .single_access()
            .unwrap()
            .descriptor()
            .metadata()
    );
    assert_eq!(
        access.descriptor().sequence_position(),
        Some(ImageSequencePosition::new(MediaId::from_raw(505), 1))
    );
    let storage = access.scanline_storage().unwrap();
    for y in storage.bounds().min_y()..storage.bounds().max_y() {
        for x in storage.bounds().min_x()..storage.bounds().max_x() {
            for channel in 0..3 {
                assert_eq!(storage.sample_bytes(channel, x, y), Some(&[0, 0][..]));
            }
            assert_eq!(
                storage.sample_bytes(3, x, y),
                Some(&u16::MAX.to_ne_bytes()[..])
            );
        }
    }
}

#[test]
fn black_policy_preserves_multipart_tiled_mip_and_region_contracts() {
    let directory = TemporaryDirectory::new("black-exr");
    let pattern =
        ImageSequencePattern::new(directory.path().to_path_buf(), "comp.", ".exr", 4).unwrap();
    let image = multipart_tiled_mip_exr_image();
    let mut writer =
        ImageSequenceWriter::new(pattern.clone(), 1, 1, WriteOptions::default()).unwrap();
    writer.write_image(&image).unwrap();
    writer.write_image(&image).unwrap();
    writer.write_image(&image).unwrap();
    fs::remove_file(pattern.path_for_frame(2).unwrap()).unwrap();

    let manifest = ImageSequenceManifest::discover(pattern.path_for_frame(1).unwrap(), 1).unwrap();
    let media_id = MediaId::from_raw(606);
    let reader = ImageSequenceReader::new(
        manifest,
        media_id,
        MissingFramePolicy::Black,
        ReadOptions::default(),
    );
    let reference = reader.read_image(0).unwrap();
    let black = reader.read_image(1).unwrap();
    assert_eq!(black.substitution(), SequenceSubstitution::Black);
    assert_eq!(black.source_frame_number(), None);
    assert_eq!(black.image().layers().len(), 2);

    for (actual, expected) in black
        .image()
        .layers()
        .iter()
        .zip(reference.image().layers())
    {
        assert_eq!(actual.name(), expected.name());
        assert_eq!(
            actual.access().descriptor().channels(),
            expected.access().descriptor().channels()
        );
        assert_eq!(
            actual.access().descriptor().sample_types(),
            expected.access().descriptor().sample_types()
        );
        assert_eq!(
            actual.access().descriptor().data_window(),
            expected.access().descriptor().data_window()
        );
        assert_eq!(
            actual.access().descriptor().display_window(),
            expected.access().descriptor().display_window()
        );
        assert_eq!(
            actual.access().descriptor().alpha_mode(),
            expected.access().descriptor().alpha_mode()
        );
        assert_eq!(
            actual.access().descriptor().color_tags(),
            expected.access().descriptor().color_tags()
        );
        assert_eq!(
            actual.access().descriptor().metadata(),
            expected.access().descriptor().metadata()
        );
        assert_eq!(
            actual.access().organization(),
            expected.access().organization()
        );
        assert_eq!(
            actual.access().tile_description(),
            expected.access().tile_description()
        );
        assert_eq!(
            actual.access().level_count(),
            expected.access().level_count()
        );
        assert_eq!(
            actual.access().descriptor().sequence_position(),
            Some(ImageSequencePosition::new(media_id, 1))
        );
    }

    let beauty = black.image().layers()[0].access();
    assert_eq!(beauty.organization(), ImageOrganization::Tiled);
    for level in beauty.levels() {
        let bounds = beauty.level_bounds(level).unwrap();
        let region = beauty.region_all(level, bounds).unwrap();
        for y in bounds.min_y()..bounds.max_y() {
            for x in bounds.min_x()..bounds.max_x() {
                assert_eq!(
                    region.sample_bytes(ChannelIndex::new(0), x, y),
                    Some(&0x3c00_u16.to_ne_bytes()[..])
                );
                assert_eq!(
                    region.sample_bytes(ChannelIndex::new(1), x, y),
                    Some(&[0, 0][..])
                );
            }
        }
    }

    let depth = black.image().layers()[1].access();
    assert_eq!(depth.organization(), ImageOrganization::Scanline);
    let bounds = depth.descriptor().data_window();
    let region = depth.region_all(MipLevel::BASE, bounds).unwrap();
    for y in bounds.min_y()..bounds.max_y() {
        for x in bounds.min_x()..bounds.max_x() {
            assert_eq!(
                region.sample_bytes(ChannelIndex::new(0), x, y),
                Some(&0.0_f32.to_ne_bytes()[..])
            );
        }
    }
}

#[test]
fn sequence_writer_refuses_existing_frames_and_only_advances_after_publish() {
    let directory = TemporaryDirectory::new("collision");
    let pattern =
        ImageSequencePattern::new(directory.path().to_path_buf(), "render.", ".png", 4).unwrap();
    directory.touch("render.0001.png");
    let mut writer = ImageSequenceWriter::new(pattern, 1, 1, WriteOptions::default()).unwrap();

    let error = writer.write_image(&rgba16_image([0; 8])).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(writer.frames_written(), 0);
}

#[test]
fn failed_sequence_encode_cleans_temporary_data_and_can_retry_the_same_number() {
    let directory = TemporaryDirectory::new("encode-retry");
    let pattern =
        ImageSequencePattern::new(directory.path().to_path_buf(), "retry.", ".png", 4).unwrap();
    let mut writer =
        ImageSequenceWriter::new(pattern.clone(), 1, 1, WriteOptions::default()).unwrap();
    let signed_bounds = PixelBounds::from_origin_size(-1, 0, 2, 1).unwrap();
    let unsupported = rgba16_image_with_bounds(signed_bounds, [0; 8]);

    let error = writer.write_image(&unsupported).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unsupported);
    assert_eq!(writer.frames_written(), 0);
    assert!(fs::read_dir(directory.path()).unwrap().next().is_none());

    let written = writer.write_image(&rgba16_image([0; 8])).unwrap();
    assert_eq!(written.image_number(), 0);
    assert_eq!(written.file_frame_number(), 1);
    assert_eq!(
        written.path(),
        Some(pattern.path_for_frame(1).unwrap().as_path())
    );
    assert_eq!(writer.frames_written(), 1);
}

#[test]
fn sequence_writer_does_not_expand_long_destination_names_for_temporary_output() {
    let directory = TemporaryDirectory::new("long-output-name");
    let pattern =
        ImageSequencePattern::new(directory.path().to_path_buf(), "x".repeat(230), ".png", 4)
            .unwrap();
    let mut writer =
        ImageSequenceWriter::new(pattern.clone(), 1, 1, WriteOptions::default()).unwrap();

    let written = writer.write_image(&rgba16_image([0; 8])).unwrap();
    assert_eq!(
        written.path(),
        Some(pattern.path_for_frame(1).unwrap().as_path())
    );
    assert_eq!(fs::read_dir(directory.path()).unwrap().count(), 1);
}
