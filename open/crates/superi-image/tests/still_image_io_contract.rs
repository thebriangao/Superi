use std::io::Cursor;
use std::sync::Arc;

use superi_core::color_space::ColorSpace;
use superi_core::error::ErrorCategory;
use superi_core::geometry::PixelBounds;
use superi_core::ids::MediaId;
use superi_core::pixel::AlphaMode;
use superi_image::channels::ChannelList;
use superi_image::io::{
    read, write, DpxEndianness, DpxPacking, ReadOptions, StillImage, StillImageFormat,
    StillImageLayer, WriteOptions,
};
use superi_image::metadata::{
    ImageMetadata, ImageMetadataFloat, ImageMetadataValue, ImageOrientation,
};
use superi_image::model::{
    ByteAlignment, ChannelSlice, ChannelStorageLayout, ImageStorage, StoragePlane,
};
use superi_image::tiling::{
    ImageAccess, ImageAccessDescriptor, ImageOrganization, ImageSequencePosition, ImageTile,
    LevelRoundingMode, MipLevel, MipMode, TileDescription, TileIndex,
};
use superi_image::value::ImageSampleType;

fn rgba16_image() -> StillImage {
    let bounds = PixelBounds::from_origin_size(0, 0, 2, 1).unwrap();
    let descriptor = ImageAccessDescriptor::new(
        bounds,
        bounds,
        ChannelList::from_full_names(["R", "G", "B", "A"]).unwrap(),
        vec![ImageSampleType::U16; 4],
        ColorSpace::SRGB,
        AlphaMode::Straight,
    )
    .unwrap()
    .with_sequence_position(ImageSequencePosition::new(MediaId::from_raw(91), 17));
    let samples = [0_u16, 65_535, 257, 32_768, 1024, 2048, 4096, 65_535];
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

fn mixed_planar_access(
    data_window: PixelBounds,
    display_window: PixelBounds,
    channels: &[&str],
    sample_types: Vec<ImageSampleType>,
    planes: Vec<Vec<u8>>,
    alpha_mode: AlphaMode,
) -> ImageAccess {
    assert_eq!(channels.len(), planes.len());
    assert_eq!(channels.len(), sample_types.len());
    let width = usize::try_from(data_window.width()).unwrap();
    let height = usize::try_from(data_window.height()).unwrap();
    let mut storage_planes = Vec::new();
    let mut slices = Vec::new();
    for (index, (sample_type, bytes)) in sample_types.iter().zip(planes).enumerate() {
        let sample_bytes = usize::from(sample_type.bits() / 8);
        assert_eq!(bytes.len(), width * height * sample_bytes);
        storage_planes.push(
            StoragePlane::new(
                Arc::from(bytes),
                0,
                width * sample_bytes,
                ByteAlignment::new(sample_bytes).unwrap(),
            )
            .unwrap(),
        );
        slices.push(ChannelSlice::new(index, 0, sample_bytes, sample_bytes).unwrap());
    }
    let descriptor = ImageAccessDescriptor::new(
        data_window,
        display_window,
        ChannelList::from_full_names(channels.iter().copied()).unwrap(),
        sample_types,
        ColorSpace::UNSPECIFIED,
        alpha_mode,
    )
    .unwrap();
    let storage = ImageStorage::new(
        data_window,
        ChannelStorageLayout::Planar,
        storage_planes,
        slices,
    )
    .unwrap();
    ImageAccess::from_scanline(descriptor, storage).unwrap()
}

fn rgb8_image(width: u32, height: u32, pixel: Option<[u8; 3]>) -> StillImage {
    let bounds = PixelBounds::from_origin_size(0, 0, width, height).unwrap();
    let mut bytes = Vec::new();
    for index in 0..width * height {
        bytes.extend_from_slice(&pixel.unwrap_or_else(|| {
            [
                (index * 31) as u8,
                (255_u32.saturating_sub(index * 17)) as u8,
                (index * 53 + 9) as u8,
            ]
        }));
    }
    let descriptor = ImageAccessDescriptor::new(
        bounds,
        bounds,
        ChannelList::from_full_names(["R", "G", "B"]).unwrap(),
        vec![ImageSampleType::U8; 3],
        ColorSpace::SRGB,
        AlphaMode::Opaque,
    )
    .unwrap();
    let storage = ImageStorage::new(
        bounds,
        ChannelStorageLayout::Interleaved,
        vec![
            StoragePlane::new(Arc::from(bytes), 0, width as usize * 3, ByteAlignment::ONE).unwrap(),
        ],
        (0..3)
            .map(|channel| ChannelSlice::new(0, channel, 1, 3).unwrap())
            .collect(),
    )
    .unwrap();
    StillImage::from_access(ImageAccess::from_scanline(descriptor, storage).unwrap())
}

fn f16_tile(bounds: PixelBounds, values: &[u16]) -> ImageStorage {
    assert_eq!(
        values.len(),
        bounds.width() as usize * bounds.height() as usize
    );
    let bytes = values
        .iter()
        .copied()
        .flat_map(u16::to_ne_bytes)
        .collect::<Vec<_>>();
    ImageStorage::new(
        bounds,
        ChannelStorageLayout::Planar,
        vec![StoragePlane::new(
            Arc::from(bytes),
            0,
            bounds.width() as usize * 2,
            ByteAlignment::new(2).unwrap(),
        )
        .unwrap()],
        vec![ChannelSlice::new(0, 0, 2, 2).unwrap()],
    )
    .unwrap()
}

#[test]
fn png_round_trip_preserves_native_precision_alpha_extent_and_sequence_identity() {
    let source = rgba16_image();
    let mut encoded = Cursor::new(Vec::new());
    write(
        &mut encoded,
        StillImageFormat::Png,
        &source,
        &WriteOptions::default(),
    )
    .unwrap();

    encoded.set_position(0);
    let decoded = read(
        &mut encoded,
        StillImageFormat::Png,
        &ReadOptions::default()
            .with_sequence_position(ImageSequencePosition::new(MediaId::from_raw(91), 17)),
    )
    .unwrap();

    let access = decoded.single_access().unwrap();
    assert_eq!(
        access.descriptor().data_window(),
        source.single_access().unwrap().descriptor().data_window()
    );
    assert_eq!(
        access.descriptor().display_window(),
        source
            .single_access()
            .unwrap()
            .descriptor()
            .display_window()
    );
    assert_eq!(
        access.descriptor().channels(),
        source.single_access().unwrap().descriptor().channels()
    );
    assert_eq!(
        access.descriptor().sample_types(),
        &[ImageSampleType::U16; 4]
    );
    assert_eq!(access.descriptor().alpha_mode(), AlphaMode::Straight);
    assert_eq!(
        access.descriptor().sequence_position(),
        source
            .single_access()
            .unwrap()
            .descriptor()
            .sequence_position()
    );
    assert_eq!(
        access.scanline_storage().unwrap(),
        source.single_access().unwrap().scanline_storage().unwrap()
    );
}

#[test]
fn exr_round_trip_preserves_multipart_channels_precision_and_signed_windows() {
    let display = PixelBounds::new(-20, -10, 40, 30).unwrap();
    let beauty_bounds = PixelBounds::from_origin_size(-3, 7, 2, 2).unwrap();
    let beauty = mixed_planar_access(
        beauty_bounds,
        display,
        &["A", "R", "objectId"],
        vec![
            ImageSampleType::F16,
            ImageSampleType::F32,
            ImageSampleType::U32,
        ],
        vec![
            [0x00_u16, 0x3c00, 0x3800, 0x3400]
                .into_iter()
                .flat_map(u16::to_ne_bytes)
                .collect(),
            [1.0_f32, -0.0, f32::INFINITY, f32::from_bits(0x7fc0_1234)]
                .into_iter()
                .flat_map(|value| value.to_bits().to_ne_bytes())
                .collect(),
            [0_u32, 1, 0x8000_0000, u32::MAX]
                .into_iter()
                .flat_map(u32::to_ne_bytes)
                .collect(),
        ],
        AlphaMode::Premultiplied,
    );
    let depth_bounds = PixelBounds::from_origin_size(12, -5, 1, 2).unwrap();
    let depth = mixed_planar_access(
        depth_bounds,
        display,
        &["Z"],
        vec![ImageSampleType::F32],
        vec![[5.0_f32, 9.0]
            .into_iter()
            .flat_map(|value| value.to_ne_bytes())
            .collect()],
        AlphaMode::Opaque,
    );
    let source = StillImage::new(vec![
        StillImageLayer::new(Some("beauty".to_owned()), beauty).unwrap(),
        StillImageLayer::new(Some("depth".to_owned()), depth).unwrap(),
    ])
    .unwrap();

    let mut encoded = Cursor::new(Vec::new());
    write(
        &mut encoded,
        StillImageFormat::Exr,
        &source,
        &WriteOptions::default(),
    )
    .unwrap();
    encoded.set_position(0);
    let decoded = read(&mut encoded, StillImageFormat::Exr, &ReadOptions::default()).unwrap();

    assert_eq!(decoded.layers().len(), 2);
    for (actual, expected) in decoded.layers().iter().zip(source.layers()) {
        assert_eq!(actual.name(), expected.name());
        assert_eq!(actual.access().descriptor(), expected.access().descriptor());
        assert_eq!(
            actual.access().scanline_storage(),
            expected.access().scanline_storage()
        );
    }
}

#[test]
fn exr_round_trip_preserves_tile_geometry_edge_tiles_and_mip_levels() {
    let bounds = PixelBounds::from_origin_size(-2, 3, 3, 3).unwrap();
    let descriptor = ImageAccessDescriptor::new(
        bounds,
        PixelBounds::new(-8, -4, 8, 12).unwrap(),
        ChannelList::from_full_names(["Y"]).unwrap(),
        vec![ImageSampleType::F16],
        ColorSpace::UNSPECIFIED,
        AlphaMode::Opaque,
    )
    .unwrap();
    let tiles = vec![
        ImageTile::new(
            MipLevel::BASE,
            TileIndex::new(0, 0),
            f16_tile(PixelBounds::new(-2, 3, 0, 5).unwrap(), &[1, 2, 4, 5]),
        ),
        ImageTile::new(
            MipLevel::BASE,
            TileIndex::new(1, 0),
            f16_tile(PixelBounds::new(0, 3, 1, 5).unwrap(), &[3, 6]),
        ),
        ImageTile::new(
            MipLevel::BASE,
            TileIndex::new(0, 1),
            f16_tile(PixelBounds::new(-2, 5, 0, 6).unwrap(), &[7, 8]),
        ),
        ImageTile::new(
            MipLevel::BASE,
            TileIndex::new(1, 1),
            f16_tile(PixelBounds::new(0, 5, 1, 6).unwrap(), &[9]),
        ),
        ImageTile::new(
            MipLevel::new(1),
            TileIndex::new(0, 0),
            f16_tile(PixelBounds::new(-2, 3, -1, 4).unwrap(), &[10]),
        ),
    ];
    let access = ImageAccess::tiled(
        descriptor,
        TileDescription::new(2, 2, MipMode::Mipmap, LevelRoundingMode::Down).unwrap(),
        tiles,
    )
    .unwrap();
    let source = StillImage::from_access(access);
    let mut encoded = Cursor::new(Vec::new());
    write(
        &mut encoded,
        StillImageFormat::Exr,
        &source,
        &WriteOptions::default(),
    )
    .unwrap();
    encoded.set_position(0);
    let decoded = read(&mut encoded, StillImageFormat::Exr, &ReadOptions::default()).unwrap();
    let actual = decoded.single_access().unwrap();
    assert_eq!(actual.organization(), ImageOrganization::Tiled);
    assert_eq!(actual, source.single_access().unwrap());
}

#[test]
fn exr_round_trip_preserves_typed_and_arbitrary_metadata_values() {
    let bounds = PixelBounds::from_origin_size(0, 0, 1, 1).unwrap();
    let mut metadata = ImageMetadata::new().with_orientation(ImageOrientation::RightTop);
    metadata
        .insert("enabled", ImageMetadataValue::Boolean(true))
        .unwrap();
    metadata
        .insert("camera.µ", ImageMetadataValue::Text("lens 😀".to_owned()))
        .unwrap();
    metadata
        .insert("signed", ImageMetadataValue::Signed(i64::MIN + 9))
        .unwrap();
    metadata
        .insert("unsigned", ImageMetadataValue::Unsigned(u64::MAX - 7))
        .unwrap();
    metadata
        .insert(
            "float",
            ImageMetadataValue::Float(ImageMetadataFloat::from_bits(0x7ff8_0000_0000_1234)),
        )
        .unwrap();
    metadata
        .insert(
            "payload",
            ImageMetadataValue::Bytes(Arc::from([0_u8, 1, 2, 255])),
        )
        .unwrap();
    metadata
        .insert(
            "exr.pixel_aspect_bits",
            ImageMetadataValue::Unsigned(u64::from(1.25_f32.to_bits())),
        )
        .unwrap();
    let descriptor = ImageAccessDescriptor::new(
        bounds,
        bounds,
        ChannelList::from_full_names(["Z"]).unwrap(),
        vec![ImageSampleType::F32],
        ColorSpace::UNSPECIFIED,
        AlphaMode::Opaque,
    )
    .unwrap()
    .with_image_metadata(metadata.clone());
    let storage = ImageStorage::new(
        bounds,
        ChannelStorageLayout::Planar,
        vec![StoragePlane::new(
            Arc::from(3.5_f32.to_ne_bytes()),
            0,
            4,
            ByteAlignment::new(4).unwrap(),
        )
        .unwrap()],
        vec![ChannelSlice::new(0, 0, 4, 4).unwrap()],
    )
    .unwrap();
    let source = StillImage::from_access(ImageAccess::from_scanline(descriptor, storage).unwrap());
    let mut encoded = Cursor::new(Vec::new());
    write(
        &mut encoded,
        StillImageFormat::Exr,
        &source,
        &WriteOptions::default(),
    )
    .unwrap();
    encoded.set_position(0);
    let decoded = read(&mut encoded, StillImageFormat::Exr, &ReadOptions::default()).unwrap();
    assert_eq!(
        decoded.single_access().unwrap().descriptor().metadata(),
        &metadata
    );
}

#[test]
fn dpx_round_trip_preserves_ten_bit_values_byte_order_packing_and_extent() {
    let data = PixelBounds::from_origin_size(12, 8, 2, 1).unwrap();
    let display = PixelBounds::from_origin_size(0, 0, 32, 18).unwrap();
    let source = StillImage::from_access(mixed_planar_access(
        data,
        display,
        &["R", "G", "B"],
        vec![ImageSampleType::U16; 3],
        vec![
            [0_u16, 1023]
                .into_iter()
                .flat_map(u16::to_ne_bytes)
                .collect(),
            [17_u16, 511]
                .into_iter()
                .flat_map(u16::to_ne_bytes)
                .collect(),
            [901_u16, 1]
                .into_iter()
                .flat_map(u16::to_ne_bytes)
                .collect(),
        ],
        AlphaMode::Opaque,
    ));

    for (endianness, packing) in [
        (DpxEndianness::Big, DpxPacking::FilledMethodA),
        (DpxEndianness::Little, DpxPacking::FilledMethodB),
    ] {
        let options = WriteOptions::default()
            .with_dpx(endianness, packing, 10)
            .unwrap();
        let mut encoded = Cursor::new(Vec::new());
        write(&mut encoded, StillImageFormat::Dpx, &source, &options).unwrap();
        assert_eq!(
            &encoded.get_ref()[..4],
            if endianness == DpxEndianness::Big {
                b"SDPX"
            } else {
                b"XPDS"
            }
        );
        encoded.set_position(0);
        let decoded = read(&mut encoded, StillImageFormat::Dpx, &ReadOptions::default()).unwrap();
        let actual = decoded.single_access().unwrap();
        let expected = source.single_access().unwrap();
        assert_eq!(actual.descriptor().data_window(), data);
        assert_eq!(actual.descriptor().display_window(), display);
        assert_eq!(
            actual.descriptor().channels(),
            expected.descriptor().channels()
        );
        assert_eq!(
            actual.descriptor().sample_types(),
            &[ImageSampleType::U16; 3]
        );
        assert_eq!(actual.scanline_storage(), expected.scanline_storage());
    }
}

#[test]
fn extensions_cover_the_complete_still_image_format_matrix() {
    let cases = [
        ("EXR", StillImageFormat::Exr),
        ("dpx", StillImageFormat::Dpx),
        ("png", StillImageFormat::Png),
        ("jpeg", StillImageFormat::Jpeg),
        ("tif", StillImageFormat::Tiff),
        ("webp", StillImageFormat::WebP),
        ("tga", StillImageFormat::Tga),
        ("bmp", StillImageFormat::Bmp),
    ];
    for (extension, expected) in cases {
        assert_eq!(StillImageFormat::from_extension(extension), Some(expected));
    }
    assert_eq!(StillImageFormat::ALL.len(), 8);
}

#[test]
fn all_common_lossless_raster_backends_preserve_rgb8_samples() {
    let source = rgb8_image(3, 2, None);
    for format in [
        StillImageFormat::Png,
        StillImageFormat::Tiff,
        StillImageFormat::WebP,
        StillImageFormat::Tga,
        StillImageFormat::Bmp,
    ] {
        let mut encoded = Cursor::new(Vec::new());
        write(&mut encoded, format, &source, &WriteOptions::default()).unwrap();
        encoded.set_position(0);
        let decoded = read(&mut encoded, format, &ReadOptions::default()).unwrap();
        assert_eq!(
            decoded.single_access().unwrap().scanline_storage(),
            source.single_access().unwrap().scanline_storage(),
            "{format:?} changed lossless RGB samples"
        );
    }
}

#[test]
fn jpeg_backend_retains_channel_meaning_and_bounded_lossy_values() {
    let source = rgb8_image(16, 16, Some([41, 97, 173]));
    let mut encoded = Cursor::new(Vec::new());
    write(
        &mut encoded,
        StillImageFormat::Jpeg,
        &source,
        &WriteOptions::default().with_jpeg_quality(100).unwrap(),
    )
    .unwrap();
    encoded.set_position(0);
    let decoded = read(
        &mut encoded,
        StillImageFormat::Jpeg,
        &ReadOptions::default(),
    )
    .unwrap();
    let access = decoded.single_access().unwrap();
    assert_eq!(
        access
            .descriptor()
            .channels()
            .iter()
            .map(|name| name.as_str())
            .collect::<Vec<_>>(),
        ["R", "G", "B"]
    );
    let storage = access.scanline_storage().unwrap();
    for (channel, expected) in [41_u8, 97, 173].into_iter().enumerate() {
        let actual = storage.sample_bytes(channel, 0, 0).unwrap()[0];
        assert!(
            actual.abs_diff(expected) <= 3,
            "channel {channel}: {actual}"
        );
    }
}

#[test]
fn dpx_round_trip_covers_every_supported_integer_depth() {
    for bit_depth in [8_u8, 12, 16] {
        let sample_type = if bit_depth == 8 {
            ImageSampleType::U8
        } else {
            ImageSampleType::U16
        };
        let max = if bit_depth == 16 {
            u16::MAX
        } else {
            ((1_u32 << bit_depth) - 1) as u16
        };
        let planes = [vec![0_u16, max], vec![max / 3, max / 2], vec![max - 1, 1]]
            .into_iter()
            .map(|values| {
                if bit_depth == 8 {
                    values.into_iter().map(|value| value as u8).collect()
                } else {
                    values.into_iter().flat_map(u16::to_ne_bytes).collect()
                }
            })
            .collect();
        let bounds = PixelBounds::from_origin_size(0, 0, 2, 1).unwrap();
        let source = StillImage::from_access(mixed_planar_access(
            bounds,
            bounds,
            &["R", "G", "B"],
            vec![sample_type; 3],
            planes,
            AlphaMode::Opaque,
        ));
        let options = WriteOptions::default()
            .with_dpx(DpxEndianness::Little, DpxPacking::FilledMethodB, bit_depth)
            .unwrap();
        let mut encoded = Cursor::new(Vec::new());
        write(&mut encoded, StillImageFormat::Dpx, &source, &options).unwrap();
        encoded.set_position(0);
        let decoded = read(&mut encoded, StillImageFormat::Dpx, &ReadOptions::default()).unwrap();
        assert_eq!(
            decoded.single_access().unwrap().scanline_storage(),
            source.single_access().unwrap().scanline_storage(),
            "{bit_depth}-bit DPX changed samples"
        );
    }
}

#[test]
fn decode_limits_and_malformed_data_return_stable_error_categories() {
    let source = rgb8_image(3, 2, None);
    let mut encoded = Cursor::new(Vec::new());
    write(
        &mut encoded,
        StillImageFormat::Png,
        &source,
        &WriteOptions::default(),
    )
    .unwrap();
    encoded.set_position(0);
    let limited = read(
        &mut encoded,
        StillImageFormat::Png,
        &ReadOptions::new(2, 2, 1024).unwrap(),
    )
    .unwrap_err();
    assert_eq!(limited.category(), ErrorCategory::ResourceExhausted);

    let mut malformed = Cursor::new(b"not an image".to_vec());
    let corrupt = read(
        &mut malformed,
        StillImageFormat::Png,
        &ReadOptions::default(),
    )
    .unwrap_err();
    assert_eq!(corrupt.category(), ErrorCategory::CorruptData);
}

#[test]
fn raster_writes_reject_unrepresentable_signed_windows_without_conversion() {
    let bounds = PixelBounds::from_origin_size(-1, 0, 1, 1).unwrap();
    let source = StillImage::from_access(mixed_planar_access(
        bounds,
        bounds,
        &["R", "G", "B"],
        vec![ImageSampleType::U8; 3],
        vec![vec![1], vec![2], vec![3]],
        AlphaMode::Opaque,
    ));
    let error = write(
        &mut Cursor::new(Vec::new()),
        StillImageFormat::Png,
        &source,
        &WriteOptions::default(),
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unsupported);
}
