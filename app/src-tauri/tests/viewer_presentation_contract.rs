use superi_color::gpu_display::GpuDisplayView;
use superi_core::color_space::ColorSpace;
use superi_core::pixel::AlphaMode;
use superi_desktop::viewport::{
    DesktopViewerAnalysisView, DesktopViewerRole, DesktopViewportSurfaceDestination,
    ViewerDisplayTransform, ViewerPresentationIntent,
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
fn inline_and_external_outputs_share_the_native_presentation_owner() {
    assert_eq!(
        DesktopViewportSurfaceDestination::ALL,
        &[
            DesktopViewportSurfaceDestination::Inline,
            DesktopViewportSurfaceDestination::External,
        ]
    );
    assert_eq!(DesktopViewportSurfaceDestination::Inline.code(), "inline");
    assert_eq!(
        DesktopViewportSurfaceDestination::External.code(),
        "external"
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

#[test]
fn built_in_viewer_transforms_preserve_scene_meaning_and_change_the_real_display_branch() {
    let extent = wgpu::Extent3d {
        width: 3_840,
        height: 2_160,
        depth_or_array_layers: 1,
    };
    assert_eq!(
        ViewerDisplayTransform::ALL,
        &[
            ViewerDisplayTransform::Srgb,
            ViewerDisplayTransform::DisplayP3
        ]
    );

    let srgb = ViewerPresentationIntent::for_display(
        DesktopViewerRole::Program,
        extent,
        ViewerDisplayTransform::Srgb,
    )
    .unwrap();
    let p3 = ViewerPresentationIntent::for_display(
        DesktopViewerRole::Program,
        extent,
        ViewerDisplayTransform::DisplayP3,
    )
    .unwrap();

    assert_eq!(srgb.scene_space(), ColorSpace::ACESCG);
    assert_eq!(p3.scene_space(), ColorSpace::ACESCG);
    assert_eq!(srgb.source_format(), wgpu::TextureFormat::Rgba16Float);
    assert_eq!(p3.source_format(), wgpu::TextureFormat::Rgba16Float);
    assert_eq!(srgb.display_space(), ColorSpace::SRGB);
    assert_eq!(p3.display_space(), ColorSpace::DISPLAY_P3);
    assert_eq!(srgb.display_transform_code(), "srgb");
    assert_eq!(p3.display_transform_code(), "display_p3");
    assert_eq!(
        p3.display_transform_id(),
        "superi.viewport.acescg-to-display-p3.v1"
    );
    assert_eq!(srgb.transform_order(), p3.transform_order());
    assert_ne!(srgb, p3);
    assert_eq!(
        p3,
        ViewerPresentationIntent::for_display(
            DesktopViewerRole::Program,
            extent,
            ViewerDisplayTransform::DisplayP3,
        )
        .unwrap()
    );
}
