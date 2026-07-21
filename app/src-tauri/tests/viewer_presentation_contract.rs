use superi_color::gpu_display::GpuDisplayView;
use superi_core::color_space::ColorSpace;
use superi_core::pixel::AlphaMode;
use superi_desktop::viewport::{
    DesktopViewerAnalysisView, DesktopViewerRole, ViewerPresentationIntent,
};
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

#[test]
fn viewer_analysis_codes_map_exhaustively_to_their_color_stage() {
    assert_eq!(
        DesktopViewerAnalysisView::ALL,
        &[
            DesktopViewerAnalysisView::Image,
            DesktopViewerAnalysisView::Alpha,
            DesktopViewerAnalysisView::Red,
            DesktopViewerAnalysisView::Green,
            DesktopViewerAnalysisView::Blue,
            DesktopViewerAnalysisView::Luminance,
            DesktopViewerAnalysisView::FalseColor,
            DesktopViewerAnalysisView::Clipping,
        ]
    );

    let expected = [
        (
            DesktopViewerAnalysisView::Image,
            GpuDisplayView::Image,
            "image",
            "source_scene_linear",
        ),
        (
            DesktopViewerAnalysisView::Alpha,
            GpuDisplayView::Alpha,
            "alpha",
            "source_scene_linear",
        ),
        (
            DesktopViewerAnalysisView::Red,
            GpuDisplayView::Red,
            "red",
            "source_scene_linear",
        ),
        (
            DesktopViewerAnalysisView::Green,
            GpuDisplayView::Green,
            "green",
            "source_scene_linear",
        ),
        (
            DesktopViewerAnalysisView::Blue,
            GpuDisplayView::Blue,
            "blue",
            "source_scene_linear",
        ),
        (
            DesktopViewerAnalysisView::Luminance,
            GpuDisplayView::Luminance,
            "luminance",
            "source_scene_linear",
        ),
        (
            DesktopViewerAnalysisView::FalseColor,
            GpuDisplayView::FalseColor,
            "false_color",
            "source_scene_linear",
        ),
        (
            DesktopViewerAnalysisView::Clipping,
            GpuDisplayView::Clipping,
            "clipping",
            "display_linear",
        ),
    ];
    for (desktop, gpu, code, stage) in expected {
        assert_eq!(desktop.code(), code);
        assert_eq!(desktop.gpu_view(), gpu);
        assert_eq!(gpu.analysis_stage(), stage);
    }
}
