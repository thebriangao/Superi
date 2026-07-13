use superi_core::error::{ErrorCategory, Recoverability};
use superi_gpu::device::{
    AdapterSelection, DeviceRequest, GpuDevice, GpuInstance, InstanceOptions,
};
use superi_gpu::resource::{GpuResourceId, GpuResources};
use superi_gpu::texture_pool::{TextureAlignment, TexturePool, TexturePoolConfig, TextureRequest};
use superi_gpu::wgpu;

fn assert_send_sync<T: Send + Sync>() {}

fn test_device() -> Option<GpuDevice> {
    let instance = GpuInstance::new(InstanceOptions::default()).ok()?;
    let adapter = instance
        .enumerate_adapters()
        .select(&AdapterSelection::default())
        .ok()?;
    pollster::block_on(
        adapter.create_device(&DeviceRequest::default().with_label("superi texture pool contract")),
    )
    .ok()
}

fn size(width: u32, height: u32) -> wgpu::Extent3d {
    wgpu::Extent3d {
        width,
        height,
        depth_or_array_layers: 1,
    }
}

fn basic_request(width: u32, height: u32) -> TextureRequest {
    TextureRequest::new(
        size(width, height),
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    )
}

fn working_request(width: u32, height: u32) -> TextureRequest {
    TextureRequest::new(
        size(width, height),
        wgpu::TextureFormat::Rgba16Float,
        wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
    )
}

#[test]
fn allocation_extent_combines_reuse_granularity_and_format_requirements() {
    let alignment = TextureAlignment::new(6, 10).unwrap();
    let request = TextureRequest::new(
        size(13, 21),
        wgpu::TextureFormat::Bc1RgbaUnorm,
        wgpu::TextureUsages::TEXTURE_BINDING,
    )
    .with_alignment(alignment);

    assert_eq!(request.allocation_size().unwrap(), size(24, 40));
    assert_eq!(request.logical_size(), size(13, 21));

    let zero = TextureAlignment::new(0, 8).unwrap_err();
    assert_eq!(zero.category(), ErrorCategory::InvalidInput);
    assert_eq!(zero.recoverability(), Recoverability::UserCorrectable);

    let overflow = basic_request(u32::MAX, 1)
        .with_alignment(TextureAlignment::new(2, 1).unwrap())
        .allocation_size()
        .unwrap_err();
    assert_eq!(overflow.category(), ErrorCategory::ResourceExhausted);
    assert_eq!(
        overflow.contexts().last().unwrap().operation(),
        "align_texture_extent"
    );
}

#[test]
fn compatible_logical_requests_reuse_one_physical_allocation() {
    assert_send_sync::<TexturePool<'static>>();
    assert_send_sync::<superi_gpu::texture_pool::PooledTexture<'static>>();

    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping texture reuse contract");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let pool = TexturePool::new(resources, TexturePoolConfig::new(2));
    let alignment = TextureAlignment::new(64, 32).unwrap();

    let first = pool
        .acquire(
            &working_request(63, 31)
                .with_alignment(alignment)
                .with_label("first frame"),
        )
        .unwrap();
    let first_id = first.allocation_id();
    assert_eq!(first.logical_size(), size(63, 31));
    assert_eq!(first.allocation_size(), size(64, 32));
    assert_eq!(first.label(), Some("first frame"));
    assert!(first.requires_full_initialization());
    drop(first);

    let idle = pool.stats().unwrap();
    assert_eq!(idle.allocations(), 1);
    assert_eq!(idle.reuses(), 0);
    assert_eq!(idle.checked_out(), 0);
    assert_eq!(idle.idle(), 1);

    let second = pool
        .acquire(
            &working_request(64, 32)
                .with_alignment(alignment)
                .with_label("second frame"),
        )
        .unwrap();
    assert_eq!(second.allocation_id(), first_id);
    assert_eq!(second.logical_size(), size(64, 32));
    assert_eq!(second.label(), Some("second frame"));

    let reused = pool.stats().unwrap();
    assert_eq!(reused.allocations(), 1);
    assert_eq!(reused.reuses(), 1);
    assert_eq!(reused.checked_out(), 1);
    assert_eq!(reused.idle(), 0);
}

#[test]
fn incompatible_descriptors_and_escaped_handles_never_alias_reuse() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping texture isolation contract");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let pool = TexturePool::new(resources, TexturePoolConfig::new(1));

    let first = pool.acquire(&basic_request(8, 8)).unwrap();
    let escaped = first.texture().clone();
    let escaped_id = first.allocation_id();
    drop(first);

    let replacement = pool.acquire(&basic_request(8, 8)).unwrap();
    assert_ne!(replacement.allocation_id(), escaped_id);
    let replacement_id = replacement.allocation_id();
    drop(replacement);
    drop(escaped);

    let different_usage = pool
        .acquire(&TextureRequest::new(
            size(8, 8),
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsages::RENDER_ATTACHMENT,
        ))
        .unwrap();
    assert_ne!(different_usage.allocation_id(), replacement_id);
    drop(different_usage);

    let different_view_formats = pool
        .acquire(&basic_request(8, 8).with_view_formats([wgpu::TextureFormat::Rgba8UnormSrgb]))
        .unwrap();
    assert_ne!(different_view_formats.allocation_id(), replacement_id);
    drop(different_view_formats);

    let stats = pool.stats().unwrap();
    assert_eq!(stats.checked_out(), 0);
    assert_eq!(stats.idle(), 3);
    assert_eq!(stats.discarded(), 1);
    assert_eq!(pool.drain_idle().unwrap(), 3);
    assert_eq!(pool.stats().unwrap().idle(), 0);
}

#[test]
fn invalid_device_requests_are_classified_before_wgpu_allocation() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping texture validation contract");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let pool = TexturePool::new(resources, TexturePoolConfig::default());
    let error = pool
        .acquire(&TextureRequest::new(
            size(4, 4),
            wgpu::TextureFormat::Rgba8Unorm,
            wgpu::TextureUsages::empty(),
        ))
        .unwrap_err();

    assert_eq!(error.category(), ErrorCategory::InvalidInput);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(
        error.contexts().last().unwrap().component(),
        "superi-gpu.texture_pool"
    );
    assert_eq!(pool.stats().unwrap().allocations(), 0);
}

fn _allocation_id_is_a_stable_diagnostic_type(id: GpuResourceId) -> GpuResourceId {
    id
}
