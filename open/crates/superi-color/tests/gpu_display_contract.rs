use superi_color::gpu_display::{DisplayViewport, GpuDisplayPresenter};
use superi_color::transform_out::{OutputColorTransform, OutputTargetKind, OutputTransformOptions};
use superi_color::working_space::WorkingSpace;
use superi_core::color_space::ColorSpace;
use superi_core::error::ErrorCategory;
use superi_gpu::device::{
    AdapterSelection, DeviceRequest, GpuDevice, GpuInstance, InstanceOptions,
};
use superi_gpu::resource::GpuResources;
use superi_gpu::wgpu;

fn test_device() -> Option<GpuDevice> {
    let instance = GpuInstance::new(InstanceOptions::default()).ok()?;
    let adapter = instance
        .enumerate_adapters()
        .select(&AdapterSelection::default())
        .ok()?;
    pollster::block_on(adapter.create_device(&DeviceRequest::default())).ok()
}

fn display_transform() -> OutputColorTransform {
    OutputColorTransform::new(
        OutputTargetKind::Display,
        WorkingSpace::ACESCG,
        ColorSpace::SRGB,
        OutputTransformOptions::new(),
    )
    .unwrap()
}

#[test]
fn aspect_fit_is_resolution_independent_and_centered() {
    let fit = DisplayViewport::aspect_fit(
        wgpu::Extent3d {
            width: 7_680,
            height: 4_320,
            depth_or_array_layers: 1,
        },
        wgpu::Extent3d {
            width: 2_048,
            height: 2_048,
            depth_or_array_layers: 1,
        },
    )
    .unwrap();

    assert_eq!(fit.x(), 0.0);
    assert_eq!(fit.y(), 448.0);
    assert_eq!(fit.width(), 2_048.0);
    assert_eq!(fit.height(), 1_152.0);

    let error = DisplayViewport::aspect_fit(
        wgpu::Extent3d {
            width: 0,
            height: 1,
            depth_or_array_layers: 1,
        },
        wgpu::Extent3d {
            width: 1,
            height: 1,
            depth_or_array_layers: 1,
        },
    )
    .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::InvalidInput);
}

#[test]
fn presenter_is_derived_from_one_explicit_display_transform() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping native display presenter proof");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let reference = display_transform();

    let presenter =
        GpuDisplayPresenter::new(&resources, reference, wgpu::TextureFormat::Bgra8UnormSrgb)
            .unwrap();

    assert_eq!(presenter.reference(), reference);
    assert_eq!(
        presenter.target_format(),
        wgpu::TextureFormat::Bgra8UnormSrgb
    );
    assert_eq!(presenter.source_format(), wgpu::TextureFormat::Rgba16Float);
}
