use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{mpsc, Arc, Barrier, Mutex};

use superi_core::error::{ErrorCategory, Recoverability, Result};
use superi_gpu::device::{
    AdapterSelection, DeviceRequest, GpuDevice, GpuInstance, InstanceOptions,
};
use superi_gpu::pool::{
    GpuMemoryPool, MemoryBudget, MemoryClass, MemoryEvictionRequest, MemoryEvictor,
    MemoryPressureCause, MemoryPressureLevel, MemoryReservation,
};
use superi_gpu::resource::GpuResources;
use superi_gpu::texture_pool::{TexturePool, TexturePoolConfig, TextureRequest};
use superi_gpu::wgpu;

fn assert_send_sync<T: Send + Sync>() {}

fn test_device() -> Option<GpuDevice> {
    let instance = GpuInstance::new(InstanceOptions::default()).ok()?;
    let adapter = instance
        .enumerate_adapters()
        .select(&AdapterSelection::default())
        .ok()?;
    pollster::block_on(
        adapter.create_device(&DeviceRequest::default().with_label("superi memory pool contract")),
    )
    .ok()
}

#[derive(Default)]
struct ReservationEvictor {
    held: Mutex<Vec<MemoryReservation>>,
    calls: AtomicU64,
}

impl ReservationEvictor {
    fn hold(&self, reservation: MemoryReservation) {
        self.held.lock().unwrap().push(reservation);
    }

    fn calls(&self) -> u64 {
        self.calls.load(Ordering::Acquire)
    }
}

impl MemoryEvictor for ReservationEvictor {
    fn evict(&self, request: MemoryEvictionRequest) -> Result<u64> {
        self.calls.fetch_add(1, Ordering::Release);
        let mut released = 0_u64;
        let removed = {
            let mut held = self.held.lock().unwrap();
            let mut removed = Vec::new();
            while released < request.bytes_to_release() {
                let Some(reservation) = held.pop() else {
                    break;
                };
                released = released.checked_add(reservation.bytes()).unwrap();
                removed.push(reservation);
            }
            removed
        };
        drop(removed);
        Ok(released)
    }
}

#[test]
fn budget_validation_and_raii_accounting_are_exact() {
    assert_send_sync::<GpuMemoryPool>();
    assert_send_sync::<MemoryReservation>();

    let zero = MemoryBudget::new(0, 64).unwrap_err();
    assert_eq!(zero.category(), ErrorCategory::InvalidInput);
    let inverted = MemoryBudget::new(65, 64).unwrap_err();
    assert_eq!(inverted.category(), ErrorCategory::InvalidInput);

    let pool = GpuMemoryPool::new(MemoryBudget::new(64, 96).unwrap());
    let texture = pool.reserve(40, MemoryClass::Texture, &[]).unwrap();
    let cache = pool.reserve(16, MemoryClass::Cache, &[]).unwrap();
    let stats = pool.stats().unwrap();
    assert_eq!(stats.resident_bytes(), 56);
    assert_eq!(stats.resident_bytes_for(MemoryClass::Texture), 40);
    assert_eq!(stats.resident_bytes_for(MemoryClass::Cache), 16);
    assert_eq!(stats.pending_bytes(), 0);
    assert_eq!(stats.peak_resident_bytes(), 56);
    assert_eq!(texture.bytes(), 40);

    drop(texture);
    assert_eq!(pool.stats().unwrap().resident_bytes(), 16);
    drop(cache);
    assert_eq!(pool.stats().unwrap().resident_bytes(), 0);
}

#[test]
fn pressure_notifies_and_cooperative_eviction_precedes_hard_refusal() {
    let pool = GpuMemoryPool::new(MemoryBudget::new(64, 96).unwrap());
    let evictor = ReservationEvictor::default();
    evictor.hold(pool.reserve(64, MemoryClass::Cache, &[]).unwrap());
    let pressure = pool.subscribe();

    let texture = pool.reserve(40, MemoryClass::Texture, &[&evictor]).unwrap();
    assert_eq!(evictor.calls(), 1);
    let stats = pool.stats().unwrap();
    assert_eq!(stats.resident_bytes(), 40);
    assert_eq!(stats.eviction_calls(), 1);
    assert_eq!(stats.evicted_bytes(), 64);
    let event = pressure
        .try_recv()
        .expect("soft pressure must be observable");
    assert_eq!(event.level(), MemoryPressureLevel::Critical);
    assert_eq!(event.cause(), MemoryPressureCause::BudgetThreshold);
    assert_eq!(event.bytes_to_release(), 40);

    let error = pool.reserve(64, MemoryClass::Texture, &[]).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
    assert_eq!(error.recoverability(), Recoverability::Retryable);
    assert_eq!(pool.stats().unwrap().resident_bytes(), 40);
    assert_eq!(pool.stats().unwrap().pending_bytes(), 0);
    drop(texture);
}

#[test]
fn explicit_external_pressure_uses_the_same_ordered_eviction_contract() {
    let pool = GpuMemoryPool::new(MemoryBudget::new(64, 128).unwrap());
    let first = ReservationEvictor::default();
    let second = ReservationEvictor::default();
    first.hold(pool.reserve(32, MemoryClass::Cache, &[]).unwrap());
    second.hold(pool.reserve(32, MemoryClass::Texture, &[]).unwrap());
    let pressure = pool.subscribe();

    let outcome = pool
        .apply_external_pressure(MemoryPressureLevel::Critical, 0, &[&first, &second])
        .unwrap();
    assert_eq!(outcome.requested_bytes(), 64);
    assert_eq!(outcome.released_bytes(), 64);
    assert_eq!(outcome.resident_bytes(), 0);
    assert_eq!(first.calls(), 1);
    assert_eq!(second.calls(), 1);
    let event = pressure
        .try_recv()
        .expect("external pressure must be observable");
    assert_eq!(event.level(), MemoryPressureLevel::Critical);
    assert_eq!(event.cause(), MemoryPressureCause::External);
}

#[test]
fn unread_pressure_coalesces_to_the_latest_critical_state() {
    let pool = GpuMemoryPool::new(MemoryBudget::new(64, 128).unwrap());
    let pressure = pool.subscribe();
    let held = pool.reserve(80, MemoryClass::Texture, &[]).unwrap();
    let error = pool.reserve(64, MemoryClass::Texture, &[]).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);

    let latest = pressure
        .try_recv()
        .expect("latest unread pressure must remain observable");
    assert_eq!(latest.level(), MemoryPressureLevel::Critical);
    assert_eq!(latest.cause(), MemoryPressureCause::AllocationDenied);
    assert!(pressure.try_recv().is_none());
    drop(held);
}

#[test]
fn concurrent_reservations_never_race_past_the_hard_limit() {
    let pool = GpuMemoryPool::new(MemoryBudget::new(64, 64).unwrap());
    let barrier = Arc::new(Barrier::new(8));
    let (sender, receiver) = mpsc::channel();
    let mut workers = Vec::new();
    for _ in 0..8 {
        let pool = pool.clone();
        let barrier = Arc::clone(&barrier);
        let sender = sender.clone();
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            sender
                .send(pool.reserve(32, MemoryClass::Texture, &[]))
                .unwrap();
        }));
    }
    drop(sender);
    for worker in workers {
        worker.join().unwrap();
    }

    let mut reservations = Vec::new();
    let mut denied = 0;
    for result in receiver {
        match result {
            Ok(reservation) => reservations.push(reservation),
            Err(error) => {
                assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
                denied += 1;
            }
        }
    }
    assert_eq!(reservations.len(), 2);
    assert_eq!(denied, 6);
    let stats = pool.stats().unwrap();
    assert_eq!(stats.resident_bytes(), 64);
    assert_eq!(stats.pending_bytes(), 0);
    assert_eq!(stats.denied_reservations(), 6);
    drop(reservations);
    assert_eq!(pool.stats().unwrap().resident_bytes(), 0);
}

#[test]
fn texture_payload_accounting_handles_mips_samples_blocks_and_planes() {
    let rgba = TextureRequest::new(
        wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::TEXTURE_BINDING,
    )
    .with_mip_level_count(3)
    .with_sample_count(4);
    assert_eq!(rgba.allocation_bytes().unwrap(), 336);

    let compressed = TextureRequest::new(
        wgpu::Extent3d {
            width: 8,
            height: 8,
            depth_or_array_layers: 1,
        },
        wgpu::TextureFormat::Bc1RgbaUnorm,
        wgpu::TextureUsages::TEXTURE_BINDING,
    );
    assert_eq!(compressed.allocation_bytes().unwrap(), 32);

    let nv12 = TextureRequest::new(
        wgpu::Extent3d {
            width: 4,
            height: 4,
            depth_or_array_layers: 1,
        },
        wgpu::TextureFormat::NV12,
        wgpu::TextureUsages::TEXTURE_BINDING,
    );
    assert_eq!(nv12.allocation_bytes().unwrap(), 24);
}

#[test]
fn texture_pools_reclaim_only_idle_allocations_under_a_shared_budget() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping memory-backed texture pool contract");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let view_resources = resources.clone();
    let memory = GpuMemoryPool::new(MemoryBudget::new(64, 64).unwrap());
    let first_pool =
        TexturePool::with_memory_pool(resources.clone(), TexturePoolConfig::new(1), memory.clone());
    let second_pool =
        TexturePool::with_memory_pool(resources, TexturePoolConfig::new(1), memory.clone());
    let size = wgpu::Extent3d {
        width: 4,
        height: 4,
        depth_or_array_layers: 1,
    };
    let sampled = TextureRequest::new(
        size,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::TEXTURE_BINDING,
    );
    let first = first_pool.acquire(&sampled).unwrap();
    drop(first);
    assert_eq!(memory.stats().unwrap().resident_bytes(), 64);
    assert_eq!(first_pool.stats().unwrap().idle_bytes(), 64);

    let rendered = TextureRequest::new(
        size,
        wgpu::TextureFormat::Rgba8Unorm,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
    );
    let second = second_pool
        .acquire_with_eviction(&rendered, &[&first_pool])
        .unwrap();
    assert_eq!(memory.stats().unwrap().resident_bytes(), 64);
    assert_eq!(first_pool.stats().unwrap().idle(), 0);
    assert_eq!(first_pool.stats().unwrap().evictions(), 1);
    assert_eq!(second.allocation_bytes(), 64);

    let held_error = first_pool.acquire(&sampled).unwrap_err();
    assert_eq!(held_error.category(), ErrorCategory::ResourceExhausted);
    assert_eq!(held_error.recoverability(), Recoverability::Retryable);
    drop(second);
    assert_eq!(second_pool.drain_idle().unwrap(), 1);
    assert_eq!(memory.stats().unwrap().resident_bytes(), 0);

    let escaped_checkout = first_pool.acquire(&sampled).unwrap();
    let view = view_resources
        .create_texture_view(
            escaped_checkout.texture(),
            &wgpu::TextureViewDescriptor::default(),
        )
        .unwrap();
    drop(escaped_checkout);
    assert_eq!(memory.stats().unwrap().resident_bytes(), 64);
    assert_eq!(first_pool.stats().unwrap().idle(), 0);
    drop(view);
    assert_eq!(memory.stats().unwrap().resident_bytes(), 0);
}
