use std::sync::Arc;

use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::geometry::PixelBounds;
use superi_image::model::{
    ByteAlignment, ChannelSlice, ChannelStorageLayout, ImageStorage, StoragePlane,
};

fn bytes(values: impl Into<Vec<u8>>) -> Arc<[u8]> {
    Arc::from(values.into())
}

fn interleaved_rgba8() -> ImageStorage {
    let mut storage = vec![0_u8; 40];
    storage[8..16].copy_from_slice(&[10, 20, 30, 40, 11, 21, 31, 41]);
    storage[24..32].copy_from_slice(&[12, 22, 32, 42, 13, 23, 33, 43]);

    ImageStorage::new(
        PixelBounds::from_origin_size(-1, 3, 2, 2).unwrap(),
        ChannelStorageLayout::Interleaved,
        vec![StoragePlane::new(bytes(storage), 8, 16, ByteAlignment::new(8).unwrap()).unwrap()],
        (0..4)
            .map(|offset| ChannelSlice::new(0, offset, 1, 4).unwrap())
            .collect(),
    )
    .unwrap()
}

#[test]
fn interleaved_storage_preserves_signed_extent_strides_alignment_and_channel_order() {
    let image = interleaved_rgba8();

    assert_eq!(image.bounds(), PixelBounds::new(-1, 3, 1, 5).unwrap());
    assert_eq!(image.layout(), ChannelStorageLayout::Interleaved);
    assert_eq!(image.plane_count(), 1);
    assert_eq!(image.channel_count(), 4);
    assert_eq!(image.planes()[0].origin(), 8);
    assert_eq!(image.planes()[0].row_stride(), 16);
    assert_eq!(image.planes()[0].row_alignment().get(), 8);
    assert_eq!(image.channels()[2].plane_index(), 0);
    assert_eq!(image.channels()[2].byte_offset(), 2);
    assert_eq!(image.channels()[2].sample_bytes(), 1);
    assert_eq!(image.channels()[2].pixel_stride(), 4);

    assert_eq!(image.sample_bytes(0, -1, 3), Some(&[10][..]));
    assert_eq!(image.sample_bytes(1, 0, 3), Some(&[21][..]));
    assert_eq!(image.sample_bytes(2, -1, 4), Some(&[32][..]));
    assert_eq!(image.sample_bytes(3, 0, 4), Some(&[43][..]));
    assert_eq!(image.sample_bytes(4, -1, 3), None);
    assert_eq!(image.sample_bytes(0, 1, 3), None);
    assert_eq!(image.sample_bytes(0, -1, 5), None);

    assert_eq!(
        image.plane_row(0, 3),
        Some(&image.planes()[0].bytes()[8..24])
    );
    assert_eq!(
        image.plane_row(0, 4),
        Some(&image.planes()[0].bytes()[24..40])
    );
    assert_eq!(image.plane_row(1, 3), None);
}

#[test]
fn planar_storage_preserves_independent_precision_and_padded_rows() {
    let mut depth = vec![0_u8; 24];
    depth[8..14].copy_from_slice(&[1, 0, 2, 0, 3, 0]);
    depth[16..22].copy_from_slice(&[4, 0, 5, 0, 6, 0]);

    let mut confidence = vec![0_u8; 48];
    confidence[16..20].copy_from_slice(&0.25_f32.to_le_bytes());
    confidence[20..24].copy_from_slice(&0.5_f32.to_le_bytes());
    confidence[24..28].copy_from_slice(&0.75_f32.to_le_bytes());
    confidence[32..36].copy_from_slice(&1.0_f32.to_le_bytes());
    confidence[36..40].copy_from_slice(&1.25_f32.to_le_bytes());
    confidence[40..44].copy_from_slice(&1.5_f32.to_le_bytes());

    let image = ImageStorage::new(
        PixelBounds::from_origin_size(5, -2, 3, 2).unwrap(),
        ChannelStorageLayout::Planar,
        vec![
            StoragePlane::new(bytes(depth), 8, 8, ByteAlignment::new(8).unwrap()).unwrap(),
            StoragePlane::new(bytes(confidence), 16, 16, ByteAlignment::new(16).unwrap()).unwrap(),
        ],
        vec![
            ChannelSlice::new(0, 0, 2, 2).unwrap(),
            ChannelSlice::new(1, 0, 4, 4).unwrap(),
        ],
    )
    .unwrap();

    assert_eq!(image.layout(), ChannelStorageLayout::Planar);
    assert_eq!(image.channel_count(), 2);
    assert_eq!(image.plane_count(), 2);
    assert_eq!(image.sample_bytes(0, 7, -1), Some(&[6, 0][..]));
    assert_eq!(
        image.sample_bytes(1, 6, -2),
        Some(&0.5_f32.to_le_bytes()[..])
    );
    assert_eq!(image.channels()[0].sample_bytes(), 2);
    assert_eq!(image.channels()[1].sample_bytes(), 4);
    assert_eq!(image.planes()[0].row_stride(), 8);
    assert_eq!(image.planes()[1].row_stride(), 16);
}

#[test]
fn clones_share_immutable_plane_ownership_and_storage_is_thread_safe() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ImageStorage>();

    let image = interleaved_rgba8();
    let clone = image.clone();
    assert!(Arc::ptr_eq(
        image.planes()[0].bytes(),
        clone.planes()[0].bytes()
    ));

    let worker = std::thread::spawn(move || clone.sample_bytes(3, 0, 4).unwrap().to_vec());
    assert_eq!(worker.join().unwrap(), vec![43]);
}

#[test]
fn constructors_reject_invalid_alignment_and_stride_descriptors() {
    let alignment = ByteAlignment::new(4).unwrap();
    assert!(ByteAlignment::new(0).is_err());
    assert!(ByteAlignment::new(3).is_err());

    let misaligned_origin = StoragePlane::new(bytes(vec![0; 32]), 2, 8, alignment).unwrap_err();
    assert_eq!(misaligned_origin.category(), ErrorCategory::InvalidInput);
    assert_eq!(
        misaligned_origin.recoverability(),
        Recoverability::UserCorrectable
    );
    assert_eq!(
        misaligned_origin.contexts()[0].component(),
        "superi-image.model"
    );

    assert!(StoragePlane::new(bytes(vec![0; 32]), 4, 6, alignment).is_err());
    assert!(StoragePlane::new(bytes(Vec::new()), 0, 4, alignment).is_err());
    assert!(ChannelSlice::new(0, 0, 0, 1).is_err());
    assert!(ChannelSlice::new(0, 0, 2, 1).is_err());
}

#[test]
fn image_validation_rejects_ambiguous_or_out_of_bounds_layouts() {
    let bounds = PixelBounds::from_origin_size(0, 0, 3, 2).unwrap();
    let plane =
        StoragePlane::new(bytes(vec![0; 24]), 8, 8, ByteAlignment::new(8).unwrap()).unwrap();

    assert!(ImageStorage::new(
        bounds,
        ChannelStorageLayout::Interleaved,
        vec![plane.clone()],
        Vec::new(),
    )
    .is_err());
    assert!(ImageStorage::new(
        PixelBounds::new(0, 0, 0, 2).unwrap(),
        ChannelStorageLayout::Interleaved,
        vec![plane.clone()],
        vec![ChannelSlice::new(0, 0, 1, 1).unwrap()],
    )
    .is_err());

    let overlap = ImageStorage::new(
        bounds,
        ChannelStorageLayout::Interleaved,
        vec![plane.clone()],
        vec![
            ChannelSlice::new(0, 0, 2, 4).unwrap(),
            ChannelSlice::new(0, 1, 2, 4).unwrap(),
        ],
    )
    .unwrap_err();
    assert_eq!(overlap.category(), ErrorCategory::InvalidInput);

    assert!(ImageStorage::new(
        bounds,
        ChannelStorageLayout::Interleaved,
        vec![plane.clone(), plane.clone()],
        vec![ChannelSlice::new(0, 0, 1, 1).unwrap()],
    )
    .is_err());
    assert!(ImageStorage::new(
        bounds,
        ChannelStorageLayout::Interleaved,
        vec![plane.clone()],
        vec![
            ChannelSlice::new(0, 0, 1, 2).unwrap(),
            ChannelSlice::new(0, 1, 1, 3).unwrap(),
        ],
    )
    .is_err());

    assert!(ImageStorage::new(
        bounds,
        ChannelStorageLayout::Planar,
        vec![plane.clone(), plane.clone()],
        vec![
            ChannelSlice::new(0, 0, 1, 1).unwrap(),
            ChannelSlice::new(0, 0, 1, 1).unwrap(),
        ],
    )
    .is_err());
    assert!(ImageStorage::new(
        bounds,
        ChannelStorageLayout::Planar,
        vec![plane.clone()],
        vec![ChannelSlice::new(0, 1, 1, 1).unwrap()],
    )
    .is_err());

    let short_row =
        StoragePlane::new(bytes(vec![0; 24]), 8, 8, ByteAlignment::new(8).unwrap()).unwrap();
    assert!(ImageStorage::new(
        bounds,
        ChannelStorageLayout::Planar,
        vec![short_row],
        vec![ChannelSlice::new(0, 0, 4, 4).unwrap()],
    )
    .is_err());

    let short_buffer =
        StoragePlane::new(bytes(vec![0; 23]), 8, 8, ByteAlignment::new(8).unwrap()).unwrap();
    assert!(ImageStorage::new(
        bounds,
        ChannelStorageLayout::Planar,
        vec![short_buffer],
        vec![ChannelSlice::new(0, 0, 2, 2).unwrap()],
    )
    .is_err());
}

#[test]
fn layout_arithmetic_overflow_is_classified_as_resource_exhaustion() {
    let error = ImageStorage::new(
        PixelBounds::from_origin_size(0, 0, 2, 1).unwrap(),
        ChannelStorageLayout::Planar,
        vec![StoragePlane::new(bytes(vec![0; 8]), 0, 1, ByteAlignment::new(1).unwrap()).unwrap()],
        vec![ChannelSlice::new(0, 0, 1, usize::MAX).unwrap()],
    )
    .unwrap_err();

    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(error.contexts()[0].operation(), "create_image_storage");
}
