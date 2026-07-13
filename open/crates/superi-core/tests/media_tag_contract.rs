use std::collections::{BTreeSet, HashSet};

use superi_core::color_space::{
    ColorPrimaries, ColorRange, ColorSpace, MatrixCoefficients, TransferFunction,
};
use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::pixel::{
    AlphaMode, ChannelLayout, ChannelPosition, ChromaSubsampling, PixelFormat, PixelModel,
    PixelNumeric, PixelPacking, SampleFormat, SampleNumeric,
};

#[test]
fn pixel_formats_expose_storage_without_conflating_color_or_alpha_meaning() {
    let working = PixelFormat::Rgba16Float;
    assert_eq!(working.code(), "rgba16_float");
    assert_eq!(PixelFormat::from_code(working.code()), Some(working));
    assert_eq!(working.model(), PixelModel::Rgba);
    assert_eq!(working.numeric(), PixelNumeric::Float);
    assert_eq!(working.packing(), PixelPacking::Packed);
    assert_eq!(working.bits_per_component(), 16);
    assert_eq!(working.plane_count(), 1);
    assert_eq!(working.packed_bytes_per_pixel(), Some(8));
    assert_eq!(working.chroma_subsampling(), None);
    assert!(working.has_alpha());

    assert_eq!(AlphaMode::Opaque.code(), "opaque");
    assert_eq!(AlphaMode::Straight.code(), "straight");
    assert_eq!(AlphaMode::Premultiplied.code(), "premultiplied");
    assert_eq!(
        AlphaMode::from_code("premultiplied"),
        Some(AlphaMode::Premultiplied)
    );
    assert_eq!(AlphaMode::from_code("associated"), None);
}

#[test]
fn planar_yuv_formats_preserve_subsampling_plane_and_depth_information() {
    let planar = PixelFormat::Yuv420p10;
    assert_eq!(planar.model(), PixelModel::Yuv);
    assert_eq!(planar.numeric(), PixelNumeric::Unorm);
    assert_eq!(planar.packing(), PixelPacking::Planar);
    assert_eq!(planar.bits_per_component(), 10);
    assert_eq!(planar.plane_count(), 3);
    assert_eq!(planar.packed_bytes_per_pixel(), None);
    assert_eq!(planar.chroma_subsampling(), Some(ChromaSubsampling::Cs420));
    assert!(!planar.has_alpha());

    let semiplanar = PixelFormat::P010;
    assert_eq!(semiplanar.packing(), PixelPacking::Semiplanar);
    assert_eq!(semiplanar.plane_count(), 2);
    assert_eq!(semiplanar.bits_per_component(), 10);
    assert_eq!(
        semiplanar.chroma_subsampling(),
        Some(ChromaSubsampling::Cs420)
    );

    assert_eq!(PixelFormat::from_code("yuv420p10"), Some(planar));
    assert_eq!(PixelFormat::from_code("unknown"), None);
}

#[test]
fn sample_formats_make_numeric_storage_and_planarity_explicit() {
    let formats = [
        (
            SampleFormat::U8,
            SampleNumeric::UnsignedInteger,
            8,
            1,
            false,
        ),
        (
            SampleFormat::I16,
            SampleNumeric::SignedInteger,
            16,
            2,
            false,
        ),
        (
            SampleFormat::I24Planar,
            SampleNumeric::SignedInteger,
            24,
            3,
            true,
        ),
        (
            SampleFormat::I32Planar,
            SampleNumeric::SignedInteger,
            32,
            4,
            true,
        ),
        (SampleFormat::F32, SampleNumeric::Float, 32, 4, false),
        (SampleFormat::F64Planar, SampleNumeric::Float, 64, 8, true),
    ];

    for (format, numeric, bits, bytes, planar) in formats {
        assert_eq!(format.numeric(), numeric);
        assert_eq!(format.bits_per_sample(), bits);
        assert_eq!(format.bytes_per_sample(), bytes);
        assert_eq!(format.is_planar(), planar);
        assert_eq!(SampleFormat::from_code(format.code()), Some(format));
    }
    assert_eq!(SampleFormat::from_code("s24_native"), None);
}

#[test]
fn standard_channel_layouts_have_canonical_order_and_stable_identity() {
    assert_eq!(
        ChannelLayout::mono().positions(),
        &[ChannelPosition::FrontCenter]
    );
    assert_eq!(
        ChannelLayout::stereo().positions(),
        &[ChannelPosition::FrontLeft, ChannelPosition::FrontRight]
    );
    assert_eq!(
        ChannelLayout::surround_5_1().positions(),
        &[
            ChannelPosition::FrontLeft,
            ChannelPosition::FrontRight,
            ChannelPosition::FrontCenter,
            ChannelPosition::LowFrequency,
            ChannelPosition::BackLeft,
            ChannelPosition::BackRight,
        ]
    );
    assert_eq!(ChannelLayout::surround_7_1().len(), 8);
    assert!(!ChannelLayout::surround_7_1().is_empty());

    let mut ordered = BTreeSet::new();
    ordered.insert(ChannelLayout::stereo());
    ordered.insert(ChannelLayout::mono());
    assert_eq!(ordered.len(), 2);

    let mut hashed = HashSet::new();
    hashed.insert(ChannelLayout::stereo());
    assert!(hashed.contains(&ChannelLayout::stereo()));
}

#[test]
fn custom_channel_layouts_preserve_discrete_order_and_reject_ambiguity() {
    let layout = ChannelLayout::new([
        ChannelPosition::Discrete(2),
        ChannelPosition::Discrete(0),
        ChannelPosition::Discrete(1),
    ])
    .unwrap();
    assert_eq!(layout.len(), 3);
    assert_eq!(layout.position(0), Some(ChannelPosition::Discrete(2)));
    assert_eq!(
        layout.positions(),
        &[
            ChannelPosition::Discrete(2),
            ChannelPosition::Discrete(0),
            ChannelPosition::Discrete(1),
        ]
    );

    for error in [
        ChannelLayout::new([]).unwrap_err(),
        ChannelLayout::new([ChannelPosition::FrontLeft, ChannelPosition::FrontLeft]).unwrap_err(),
    ] {
        assert_eq!(error.category(), ErrorCategory::InvalidInput);
        assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
        assert_eq!(error.contexts()[0].component(), "superi-core.pixel");
    }
}

#[test]
fn color_spaces_keep_every_interpretation_axis_explicit() {
    assert_eq!(ColorSpace::SRGB.primaries(), ColorPrimaries::Bt709);
    assert_eq!(ColorSpace::SRGB.transfer(), TransferFunction::Srgb);
    assert_eq!(ColorSpace::SRGB.matrix(), MatrixCoefficients::Rgb);
    assert_eq!(ColorSpace::SRGB.range(), ColorRange::Full);

    assert_eq!(ColorSpace::BT709.transfer(), TransferFunction::Bt709);
    assert_ne!(ColorSpace::SRGB, ColorSpace::BT709);

    assert_eq!(
        ColorSpace::BT2020.transfer(),
        TransferFunction::Bt2020TenBit
    );
    assert_eq!(
        TransferFunction::from_code(TransferFunction::Bt2020TwelveBit.code()),
        Some(TransferFunction::Bt2020TwelveBit)
    );

    assert_eq!(ColorSpace::ACESCG.primaries(), ColorPrimaries::AcesAp1);
    assert_eq!(ColorSpace::ACESCG.transfer(), TransferFunction::Linear);
    assert_eq!(ColorSpace::ACESCG.matrix(), MatrixCoefficients::Rgb);
    assert_eq!(ColorSpace::ACESCG.range(), ColorRange::Full);

    assert_eq!(ColorSpace::BT2100_PQ.primaries(), ColorPrimaries::Bt2020);
    assert_eq!(ColorSpace::BT2100_PQ.transfer(), TransferFunction::Pq);
    assert_eq!(ColorSpace::BT2100_HLG.transfer(), TransferFunction::Hlg);
}

#[test]
fn unusual_color_metadata_is_preserved_instead_of_silently_normalized() {
    let source = ColorSpace::new(
        ColorPrimaries::DisplayP3,
        TransferFunction::Pq,
        MatrixCoefficients::Bt2020Constant,
        ColorRange::Limited,
    );
    assert_eq!(source.primaries(), ColorPrimaries::DisplayP3);
    assert_eq!(source.transfer(), TransferFunction::Pq);
    assert_eq!(source.matrix(), MatrixCoefficients::Bt2020Constant);
    assert_eq!(source.range(), ColorRange::Limited);

    assert_eq!(
        ColorPrimaries::from_code(ColorPrimaries::AcesAp1.code()),
        Some(ColorPrimaries::AcesAp1)
    );
    assert_eq!(
        TransferFunction::from_code("hlg"),
        Some(TransferFunction::Hlg)
    );
    assert_eq!(
        MatrixCoefficients::from_code("rgb"),
        Some(MatrixCoefficients::Rgb)
    );
    assert_eq!(ColorRange::from_code("limited"), Some(ColorRange::Limited));
}

#[test]
fn shared_media_tags_are_safe_to_inspect_across_process_owners() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<PixelFormat>();
    assert_send_sync::<AlphaMode>();
    assert_send_sync::<SampleFormat>();
    assert_send_sync::<ChannelLayout>();
    assert_send_sync::<ColorSpace>();
}

#[test]
fn every_closed_public_tag_has_a_unique_round_tripping_code() {
    fn assert_codes<T: Copy + Eq + std::hash::Hash>(
        values: &[T],
        code: impl Fn(T) -> &'static str,
        from_code: impl Fn(&str) -> Option<T>,
    ) {
        let mut codes = HashSet::new();
        for value in values {
            assert!(codes.insert(code(*value)));
            assert!(from_code(code(*value)) == Some(*value));
        }
    }

    assert_codes(PixelFormat::ALL, PixelFormat::code, PixelFormat::from_code);
    assert_codes(AlphaMode::ALL, AlphaMode::code, AlphaMode::from_code);
    assert_codes(
        SampleFormat::ALL,
        SampleFormat::code,
        SampleFormat::from_code,
    );
    assert_codes(
        ColorPrimaries::ALL,
        ColorPrimaries::code,
        ColorPrimaries::from_code,
    );
    assert_codes(
        TransferFunction::ALL,
        TransferFunction::code,
        TransferFunction::from_code,
    );
    assert_codes(
        MatrixCoefficients::ALL,
        MatrixCoefficients::code,
        MatrixCoefficients::from_code,
    );
    assert_codes(ColorRange::ALL, ColorRange::code, ColorRange::from_code);
}
