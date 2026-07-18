use superi_core::color_space::ColorSpace;
use superi_core::pixel::AlphaMode;
use superi_desktop::viewport::{DesktopViewerRole, ViewerPresentationIntent};
use superi_gpu::wgpu;

#[test]
fn four_viewers_share_one_reproducible_scene_to_display_contract() {
    assert_eq!(
        DesktopViewerRole::ALL,
        &[
            DesktopViewerRole::Source,
            DesktopViewerRole::Program,
            DesktopViewerRole::Composite,
            DesktopViewerRole::Color,
        ]
    );

    let extent = wgpu::Extent3d {
        width: 7_680,
        height: 4_320,
        depth_or_array_layers: 1,
    };
    for role in DesktopViewerRole::ALL {
        let intent = ViewerPresentationIntent::canonical(*role, extent).unwrap();
        assert_eq!(intent.role(), *role);
        assert_eq!(intent.source_extent(), extent);
        assert_eq!(intent.source_format(), wgpu::TextureFormat::Rgba16Float);
        assert_eq!(intent.alpha_mode(), AlphaMode::Premultiplied);
        assert_eq!(intent.scene_space(), ColorSpace::ACESCG);
        assert_eq!(intent.display_space(), ColorSpace::SRGB);
        assert_eq!(intent.display_target(), "display");
        assert_eq!(
            intent.transform_order(),
            &[
                "alpha_unassociate",
                "scene_to_display_primaries",
                "gamut_mapping",
                "tone_mapping",
                "transfer_encoding",
                "alpha_reassociate",
            ]
        );
        assert_eq!(intent.scene_stage_count(), 0);
        assert_eq!(intent.display_stage_kind(), "display");
        assert_eq!(
            intent.display_transform_id(),
            "superi.viewport.acescg-to-srgb.v1"
        );
        assert_eq!(
            intent,
            ViewerPresentationIntent::canonical(*role, extent).unwrap()
        );
    }

    let invalid = ViewerPresentationIntent::canonical(
        DesktopViewerRole::Program,
        wgpu::Extent3d {
            width: 0,
            height: 1,
            depth_or_array_layers: 1,
        },
    )
    .unwrap_err();
    assert_eq!(
        invalid.category(),
        superi_core::error::ErrorCategory::InvalidInput
    );
}
