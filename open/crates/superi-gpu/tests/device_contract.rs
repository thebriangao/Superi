use superi_core::error::ErrorCategory;
use superi_gpu::device::{
    AdapterCatalog, AdapterSelection, Backends, DeviceRequest, Features, GpuDevice, GpuInstance,
    InstanceOptions, Limits, SelectedAdapter,
};

fn permissive_selection() -> AdapterSelection {
    AdapterSelection::default()
        .with_required_limits(Limits::downlevel_webgl2_defaults())
        .allow_software_adapter(true)
        .require_webgpu_compliance(false)
}

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn native_owners_are_thread_safe_and_empty_backend_configuration_is_actionable() {
    assert_send_sync::<GpuInstance>();
    assert_send_sync::<AdapterCatalog>();
    assert_send_sync::<SelectedAdapter>();
    assert_send_sync::<GpuDevice>();

    let error = GpuInstance::new(InstanceOptions::new(Backends::empty())).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unsupported);
    assert_eq!(
        error.contexts().last().unwrap().operation(),
        "create_instance"
    );
}

#[test]
fn enumeration_exposes_identity_and_capabilities_then_selects_the_exact_adapter() {
    let instance = GpuInstance::new(InstanceOptions::default()).unwrap();
    let catalog = instance.enumerate_adapters();
    let Some(first) = catalog.snapshots().next().cloned() else {
        return;
    };

    assert!(!first.info().name.is_empty());
    assert!(first.capabilities().limits().max_texture_dimension_2d > 0);
    assert_eq!(
        first.capabilities().is_webgpu_compliant(),
        first.capabilities().downlevel().is_webgpu_compliant()
    );

    let expected_id = first.id();
    let selected = catalog
        .select(&permissive_selection().with_preferred_adapter(expected_id))
        .unwrap();

    assert_eq!(selected.snapshot().id(), expected_id);
    assert_eq!(
        selected.snapshot().capabilities().features(),
        first.capabilities().features()
    );
}

#[test]
fn unsupported_device_requirements_fail_before_wgpu_device_creation() {
    let instance = GpuInstance::new(InstanceOptions::default()).unwrap();
    let catalog = instance.enumerate_adapters();
    let Some(first) = catalog.snapshots().next().cloned() else {
        return;
    };
    let missing = Features::all() - first.capabilities().features();
    if missing.is_empty() {
        return;
    }

    let selection_error = catalog
        .select(&permissive_selection().with_required_features(missing))
        .unwrap_err();
    assert_eq!(selection_error.category(), ErrorCategory::Unsupported);
    assert_eq!(
        selection_error.contexts().last().unwrap().operation(),
        "select_adapter"
    );

    let catalog = instance.enumerate_adapters();
    let selected = catalog
        .select(&permissive_selection().with_preferred_adapter(first.id()))
        .unwrap();
    let request = DeviceRequest::default().with_required_features(missing);
    let error = pollster::block_on(selected.create_device(&request)).unwrap_err();

    assert_eq!(error.category(), ErrorCategory::Unsupported);
    assert_eq!(
        error.contexts().last().unwrap().operation(),
        "create_device"
    );
}

#[test]
fn selected_native_adapter_creates_an_owned_device_and_queue() {
    let instance = GpuInstance::new(InstanceOptions::default()).unwrap();
    let catalog = instance.enumerate_adapters();
    if catalog.is_empty() {
        return;
    }

    let selected = catalog.select(&AdapterSelection::default()).unwrap();
    let expected = selected.snapshot().clone();
    let request = DeviceRequest::default().with_label("superi-gpu-contract");
    let device = pollster::block_on(selected.create_device(&request)).unwrap();

    assert_eq!(device.adapter(), &expected);
    assert_eq!(device.enabled_features(), request.required_features());
    assert_eq!(device.enabled_limits(), request.required_limits());
    assert_eq!(device.label(), Some("superi-gpu-contract"));
    assert_eq!(device.wgpu_device().features(), request.required_features());
    println!(
        "created device on {} through {}",
        device.adapter().info().name,
        device.adapter().info().backend
    );
}
