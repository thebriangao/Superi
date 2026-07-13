use std::sync::Arc;

use superi_core::color_space::ColorSpace;
use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::geometry::PixelBounds;
use superi_core::ids::MediaId;
use superi_core::pixel::AlphaMode;
use superi_image::channels::{ChannelIndex, ChannelList};
use superi_image::metadata::{ImageColorTags, ImageMetadata, ImageMetadataValue, ImageOrientation};
use superi_image::model::{
    ByteAlignment, ChannelSlice, ChannelStorageLayout, ImageStorage, StoragePlane,
};
use superi_image::tiling::{
    ImageAccess, ImageAccessDescriptor, ImageOrganization, ImageSequencePosition, ImageTile,
    LevelRoundingMode, MipLevel, MipMode, TileDescription, TileIndex,
};
use superi_image::value::ImageSampleType;

fn bytes(values: impl Into<Vec<u8>>) -> Arc<[u8]> {
    Arc::from(values.into())
}

fn descriptor(
    data_window: PixelBounds,
    channel_names: &[&str],
    sample_types: Vec<ImageSampleType>,
    alpha_mode: AlphaMode,
) -> ImageAccessDescriptor {
    ImageAccessDescriptor::new(
        data_window,
        PixelBounds::from_origin_size(-20, -10, 80, 45).unwrap(),
        ChannelList::from_full_names(channel_names.iter().copied()).unwrap(),
        sample_types,
        ColorSpace::ACESCG,
        alpha_mode,
    )
    .unwrap()
}

fn u8_tile(bounds: PixelBounds, values: &[u8]) -> ImageStorage {
    assert_eq!(
        values.len(),
        usize::try_from(bounds.width()).unwrap() * usize::try_from(bounds.height()).unwrap()
    );
    let row_stride = usize::try_from(bounds.width()).unwrap();
    ImageStorage::new(
        bounds,
        ChannelStorageLayout::Planar,
        vec![StoragePlane::new(bytes(values.to_vec()), 0, row_stride, ByteAlignment::ONE).unwrap()],
        vec![ChannelSlice::new(0, 0, 1, 1).unwrap()],
    )
    .unwrap()
}

fn mixed_planar_storage() -> ImageStorage {
    let bounds = PixelBounds::from_origin_size(-2, 5, 2, 2).unwrap();
    let red = [0x00, 0x3c, 0x00, 0x40, 0x00, 0x42, 0x00, 0x44];
    let alpha = [0x00, 0x3c, 0x00, 0x38, 0x00, 0x34, 0x00, 0x30];
    let depth = [
        1.0_f32.to_le_bytes(),
        2.0_f32.to_le_bytes(),
        3.0_f32.to_le_bytes(),
        4.0_f32.to_le_bytes(),
    ]
    .concat();
    ImageStorage::new(
        bounds,
        ChannelStorageLayout::Planar,
        vec![
            StoragePlane::new(bytes(red.to_vec()), 0, 4, ByteAlignment::new(2).unwrap()).unwrap(),
            StoragePlane::new(bytes(alpha.to_vec()), 0, 4, ByteAlignment::new(2).unwrap()).unwrap(),
            StoragePlane::new(bytes(depth), 0, 8, ByteAlignment::new(4).unwrap()).unwrap(),
        ],
        vec![
            ChannelSlice::new(0, 0, 2, 2).unwrap(),
            ChannelSlice::new(1, 0, 2, 2).unwrap(),
            ChannelSlice::new(2, 0, 4, 4).unwrap(),
        ],
    )
    .unwrap()
}

#[test]
fn scanline_regions_preserve_semantics_native_precision_and_sequence_position() {
    let data_window = PixelBounds::from_origin_size(-2, 5, 2, 2).unwrap();
    let color_tags = ImageColorTags::new(ColorSpace::ACESCG)
        .with_named_space("ACES - ACEScg")
        .unwrap()
        .with_icc_profile(bytes(vec![0x49, 0x43, 0x43]))
        .unwrap();
    let metadata = ImageMetadata::new().with_orientation(ImageOrientation::RightTop);
    let descriptor = descriptor(
        data_window,
        &["beauty.R", "beauty.A", "depth.Z"],
        vec![
            ImageSampleType::F16,
            ImageSampleType::F16,
            ImageSampleType::F32,
        ],
        AlphaMode::Straight,
    )
    .with_color_tags(color_tags.clone())
    .with_image_metadata(metadata)
    .with_metadata(
        "source.camera",
        ImageMetadataValue::Text("A camera".to_owned()),
    )
    .unwrap()
    .with_sequence_position(ImageSequencePosition::new(MediaId::from_raw(77), 42));
    let storage = mixed_planar_storage();
    let red_bytes = storage.planes()[0].bytes().clone();
    let access = ImageAccess::from_scanline(descriptor, storage).unwrap();

    assert_eq!(access.organization(), ImageOrganization::Scanline);
    assert_eq!(access.level_count(), 1);
    assert!(Arc::ptr_eq(
        access.scanline_storage().unwrap().planes()[0].bytes(),
        &red_bytes
    ));
    assert_eq!(access.level_bounds(MipLevel::BASE).unwrap(), data_window);
    assert_eq!(access.descriptor().alpha_mode(), AlphaMode::Straight);
    assert_eq!(access.descriptor().color_space(), ColorSpace::ACESCG);
    assert_eq!(access.descriptor().color_tags(), &color_tags);
    assert_eq!(
        access.descriptor().sample_type(ChannelIndex::new(2)),
        Some(ImageSampleType::F32)
    );
    assert_eq!(
        access.descriptor().metadata().get("source.camera"),
        Some(&ImageMetadataValue::Text("A camera".to_owned()))
    );
    assert_eq!(
        access.descriptor().metadata().orientation(),
        Some(ImageOrientation::RightTop)
    );
    let position = access.descriptor().sequence_position().unwrap();
    assert_eq!(position.media_id(), MediaId::from_raw(77));
    assert_eq!(position.image_number(), 42);

    let region = access
        .region_by_names(
            MipLevel::BASE,
            PixelBounds::new(-1, 5, 0, 7).unwrap(),
            ["depth.Z", "beauty.R"],
        )
        .unwrap();
    assert_eq!(region.bounds(), PixelBounds::new(-1, 5, 0, 7).unwrap());
    assert_eq!(
        region.selected_channels(),
        &[ChannelIndex::new(2), ChannelIndex::new(0)]
    );
    assert_eq!(
        region
            .channel_names()
            .map(|name| name.as_str())
            .collect::<Vec<_>>(),
        ["depth.Z", "beauty.R"]
    );
    assert_eq!(
        region.sample_bytes(ChannelIndex::new(2), -1, 5),
        Some(&2.0_f32.to_le_bytes()[..])
    );
    assert_eq!(
        region.sample_bytes(ChannelIndex::new(0), -1, 6),
        Some(&[0x00, 0x44][..])
    );
    assert_eq!(region.sample_bytes(ChannelIndex::new(1), -1, 5), None);
    assert_eq!(region.sample_bytes(ChannelIndex::new(2), -2, 5), None);

    let scanline = access
        .scanline(6, &[ChannelIndex::new(1), ChannelIndex::new(0)])
        .unwrap();
    assert_eq!(scanline.bounds(), PixelBounds::new(-2, 6, 0, 7).unwrap());
    assert_eq!(
        scanline.sample_bytes(ChannelIndex::new(1), -2, 6),
        Some(&[0x00, 0x34][..])
    );
}

#[test]
fn tiled_access_clips_edge_tiles_and_crosses_backing_allocations_without_conversion() {
    let base = PixelBounds::from_origin_size(-3, 7, 3, 3).unwrap();
    let descriptor = descriptor(base, &["Y"], vec![ImageSampleType::U8], AlphaMode::Opaque);
    let edge_bytes = bytes(vec![44]);
    let edge_storage = ImageStorage::new(
        PixelBounds::new(-1, 9, 0, 10).unwrap(),
        ChannelStorageLayout::Planar,
        vec![StoragePlane::new(edge_bytes.clone(), 0, 1, ByteAlignment::ONE).unwrap()],
        vec![ChannelSlice::new(0, 0, 1, 1).unwrap()],
    )
    .unwrap();
    let tiles = vec![
        ImageTile::new(MipLevel::BASE, TileIndex::new(1, 1), edge_storage),
        ImageTile::new(
            MipLevel::BASE,
            TileIndex::new(0, 0),
            u8_tile(PixelBounds::new(-3, 7, -1, 9).unwrap(), &[11, 12, 21, 22]),
        ),
        ImageTile::new(
            MipLevel::BASE,
            TileIndex::new(1, 0),
            u8_tile(PixelBounds::new(-1, 7, 0, 9).unwrap(), &[13, 23]),
        ),
        ImageTile::new(
            MipLevel::BASE,
            TileIndex::new(0, 1),
            u8_tile(PixelBounds::new(-3, 9, -1, 10).unwrap(), &[31, 32]),
        ),
    ];
    let access = ImageAccess::tiled(
        descriptor,
        TileDescription::new(2, 2, MipMode::SingleLevel, LevelRoundingMode::Down).unwrap(),
        tiles,
    )
    .unwrap();

    assert_eq!(access.organization(), ImageOrganization::Tiled);
    assert_eq!(
        access
            .tiles(MipLevel::BASE)
            .unwrap()
            .iter()
            .map(|tile| tile.index())
            .collect::<Vec<_>>(),
        [
            TileIndex::new(0, 0),
            TileIndex::new(1, 0),
            TileIndex::new(0, 1),
            TileIndex::new(1, 1),
        ]
    );
    let edge = access.tile(MipLevel::BASE, TileIndex::new(1, 1)).unwrap();
    assert_eq!(
        edge.storage().bounds(),
        PixelBounds::new(-1, 9, 0, 10).unwrap()
    );
    assert!(Arc::ptr_eq(edge.storage().planes()[0].bytes(), &edge_bytes));

    let region_bounds = PixelBounds::new(-2, 8, 0, 10).unwrap();
    assert_eq!(
        access
            .tiles_covering_region(MipLevel::BASE, region_bounds)
            .unwrap(),
        [
            TileIndex::new(0, 0),
            TileIndex::new(1, 0),
            TileIndex::new(0, 1),
            TileIndex::new(1, 1),
        ]
    );
    let region = access.region_all(MipLevel::BASE, region_bounds).unwrap();
    assert_eq!(
        region.sample_bytes(ChannelIndex::new(0), -2, 8),
        Some(&[22][..])
    );
    assert_eq!(
        region.sample_bytes(ChannelIndex::new(0), -1, 8),
        Some(&[23][..])
    );
    assert_eq!(
        region.sample_bytes(ChannelIndex::new(0), -2, 9),
        Some(&[32][..])
    );
    assert_eq!(
        region.sample_bytes(ChannelIndex::new(0), -1, 9),
        Some(&[44][..])
    );
}

#[test]
fn mipmaps_use_explicit_rounding_signed_origins_and_complete_level_identity() {
    let base = PixelBounds::from_origin_size(-4, 2, 5, 3).unwrap();
    let down = ImageAccess::tiled(
        descriptor(base, &["Y"], vec![ImageSampleType::U8], AlphaMode::Opaque),
        TileDescription::new(3, 2, MipMode::Mipmap, LevelRoundingMode::Down).unwrap(),
        mip_tiles(-4, 2, &[(5, 3), (2, 1), (1, 1)], 3, 2),
    )
    .unwrap();
    assert_eq!(down.level_count(), 3);
    assert_eq!(
        down.levels().collect::<Vec<_>>(),
        [MipLevel::new(0), MipLevel::new(1), MipLevel::new(2)]
    );
    assert_eq!(
        down.level_bounds(MipLevel::new(1)).unwrap(),
        PixelBounds::from_origin_size(-4, 2, 2, 1).unwrap()
    );
    assert_eq!(
        down.level_bounds(MipLevel::new(2)).unwrap(),
        PixelBounds::from_origin_size(-4, 2, 1, 1).unwrap()
    );

    let up = ImageAccess::tiled(
        descriptor(base, &["Y"], vec![ImageSampleType::U8], AlphaMode::Opaque),
        TileDescription::new(3, 2, MipMode::Mipmap, LevelRoundingMode::Up).unwrap(),
        mip_tiles(-4, 2, &[(5, 3), (3, 2), (2, 1), (1, 1)], 3, 2),
    )
    .unwrap();
    assert_eq!(up.level_count(), 4);
    assert_eq!(
        up.level_bounds(MipLevel::new(1)).unwrap(),
        PixelBounds::from_origin_size(-4, 2, 3, 2).unwrap()
    );
    assert_eq!(
        up.level_bounds(MipLevel::new(3)).unwrap(),
        PixelBounds::from_origin_size(-4, 2, 1, 1).unwrap()
    );
    assert_eq!(
        up.region_all(
            MipLevel::new(2),
            PixelBounds::from_origin_size(-4, 2, 2, 1).unwrap()
        )
        .unwrap()
        .sample_bytes(ChannelIndex::new(0), -3, 2),
        Some(&[20][..])
    );
}

#[test]
fn malformed_access_descriptors_regions_and_tile_sets_fail_actionably() {
    let base = PixelBounds::from_origin_size(0, 0, 2, 2).unwrap();
    let descriptor = descriptor(base, &["Y"], vec![ImageSampleType::U8], AlphaMode::Opaque);
    let missing = ImageAccess::tiled(
        descriptor.clone(),
        TileDescription::new(1, 1, MipMode::SingleLevel, LevelRoundingMode::Down).unwrap(),
        vec![ImageTile::new(
            MipLevel::BASE,
            TileIndex::new(0, 0),
            u8_tile(PixelBounds::new(0, 0, 1, 1).unwrap(), &[1]),
        )],
    )
    .unwrap_err();
    assert_eq!(missing.category(), ErrorCategory::InvalidInput);
    assert_eq!(missing.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(missing.contexts()[0].component(), "superi-image.tiling");
    assert_eq!(missing.contexts()[0].operation(), "create_tiled_access");

    let wrong_precision = ImageAccess::from_scanline(
        ImageAccessDescriptor::new(
            base,
            base,
            ChannelList::from_full_names(["Y"]).unwrap(),
            vec![ImageSampleType::F16],
            ColorSpace::ACESCG,
            AlphaMode::Opaque,
        )
        .unwrap(),
        u8_tile(base, &[1, 2, 3, 4]),
    )
    .unwrap_err();
    assert_eq!(wrong_precision.category(), ErrorCategory::InvalidInput);

    let access = ImageAccess::from_scanline(descriptor, u8_tile(base, &[1, 2, 3, 4])).unwrap();
    let outside = access
        .region_all(MipLevel::BASE, PixelBounds::new(-1, 0, 1, 1).unwrap())
        .unwrap_err();
    assert_eq!(outside.category(), ErrorCategory::InvalidInput);
    let duplicate = access
        .region(
            MipLevel::BASE,
            PixelBounds::new(0, 0, 1, 1).unwrap(),
            &[ChannelIndex::new(0), ChannelIndex::new(0)],
        )
        .unwrap_err();
    assert_eq!(duplicate.category(), ErrorCategory::InvalidInput);
    let tile_error = access
        .tile(MipLevel::BASE, TileIndex::new(0, 0))
        .unwrap_err();
    assert_eq!(tile_error.category(), ErrorCategory::Unsupported);
}

#[test]
fn access_contracts_are_safe_to_share_across_image_workers() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<ImageAccess>();
    assert_send_sync::<ImageAccessDescriptor>();
    assert_send_sync::<ImageTile>();
    assert_send_sync::<ImageSequencePosition>();
}

fn mip_tiles(
    min_x: i32,
    min_y: i32,
    levels: &[(u32, u32)],
    tile_width: u32,
    tile_height: u32,
) -> Vec<ImageTile> {
    let mut tiles = Vec::new();
    for (level, &(width, height)) in levels.iter().enumerate() {
        let columns = width.div_ceil(tile_width);
        let rows = height.div_ceil(tile_height);
        for tile_y in 0..rows {
            for tile_x in 0..columns {
                let local_x = tile_x * tile_width;
                let local_y = tile_y * tile_height;
                let stored_width = tile_width.min(width - local_x);
                let stored_height = tile_height.min(height - local_y);
                let bounds = PixelBounds::from_origin_size(
                    min_x + i32::try_from(local_x).unwrap(),
                    min_y + i32::try_from(local_y).unwrap(),
                    stored_width,
                    stored_height,
                )
                .unwrap();
                let value = u8::try_from(level).unwrap() * 10;
                let sample_count = usize::try_from(stored_width).unwrap()
                    * usize::try_from(stored_height).unwrap();
                tiles.push(ImageTile::new(
                    MipLevel::new(u32::try_from(level).unwrap()),
                    TileIndex::new(tile_x, tile_y),
                    u8_tile(bounds, &vec![value; sample_count]),
                ));
            }
        }
    }
    tiles
}
