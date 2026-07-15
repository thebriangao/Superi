use std::sync::{mpsc, Arc, Barrier};

use superi_cache::eviction::{
    CacheBudgetLimit, CacheBudgetManager, CacheBudgetReservation, CacheCost, CacheMemoryBudgets,
};
use superi_core::error::{ErrorCategory, Recoverability};
use superi_core::ids::{DeviceId, ProjectId};
use superi_gpu::pool::{GpuMemoryPool, MemoryBudget, MemoryClass};

fn limit(bytes: u64, frames: u64) -> CacheBudgetLimit {
    CacheBudgetLimit::new(bytes, frames).unwrap()
}

fn budgets(total: (u64, u64), project: (u64, u64), device: (u64, u64)) -> CacheMemoryBudgets {
    CacheMemoryBudgets::new(
        limit(total.0, total.1),
        limit(project.0, project.1),
        limit(device.0, device.1),
    )
    .unwrap()
}

fn cost(bytes: u64, frames: u64) -> CacheCost {
    CacheCost::new(bytes, frames).unwrap()
}

fn assert_send_sync<T: Send + Sync>() {}

#[test]
fn limits_and_costs_are_checked_and_hierarchical() {
    let zero_bytes = CacheBudgetLimit::new(0, 1).unwrap_err();
    assert_eq!(zero_bytes.category(), ErrorCategory::InvalidInput);
    assert_eq!(zero_bytes.recoverability(), Recoverability::UserCorrectable);
    assert!(CacheBudgetLimit::new(1, 0).is_err());
    assert!(CacheCost::new(0, 1).is_err());
    assert!(CacheCost::new(1, 0).is_err());

    let total = limit(100, 10);
    let oversized_project = CacheMemoryBudgets::new(total, limit(101, 10), total).unwrap_err();
    assert_eq!(oversized_project.category(), ErrorCategory::InvalidInput);
    let oversized_device = CacheMemoryBudgets::new(total, total, limit(100, 11)).unwrap_err();
    assert_eq!(oversized_device.category(), ErrorCategory::InvalidInput);

    let configured = budgets((100, 10), (60, 4), (40, 3));
    assert_eq!(configured.total().max_bytes(), 100);
    assert_eq!(configured.total().max_frames(), 10);
    assert_eq!(configured.per_project().max_bytes(), 60);
    assert_eq!(configured.per_project().max_frames(), 4);
    assert_eq!(configured.per_device().max_bytes(), 40);
    assert_eq!(configured.per_device().max_frames(), 3);
}

#[test]
fn host_admission_enforces_byte_frame_and_project_limits_exactly() {
    let first_project = ProjectId::from_raw(1);
    let second_project = ProjectId::from_raw(2);
    let manager = CacheBudgetManager::new(budgets((100, 3), (60, 2), (100, 3)));

    let first = manager.reserve_host(first_project, cost(60, 1)).unwrap();
    let project_error = manager.reserve_host(first_project, cost(1, 1)).unwrap_err();
    assert_eq!(project_error.category(), ErrorCategory::ResourceExhausted);
    assert_eq!(project_error.recoverability(), Recoverability::Retryable);
    assert_eq!(
        project_error.contexts().last().unwrap().field("limit"),
        Some("project_bytes")
    );

    let second = manager.reserve_host(second_project, cost(40, 2)).unwrap();
    let byte_error = manager
        .reserve_host(ProjectId::from_raw(3), cost(1, 1))
        .unwrap_err();
    assert_eq!(
        byte_error.contexts().last().unwrap().field("limit"),
        Some("total_bytes")
    );

    let stats = manager.stats().unwrap();
    assert_eq!(stats.total_usage().bytes(), 100);
    assert_eq!(stats.total_usage().frames(), 3);
    assert_eq!(stats.project_usage(first_project).bytes(), 60);
    assert_eq!(stats.project_usage(second_project).frames(), 2);
    assert_eq!(stats.active_projects(), 2);
    assert_eq!(stats.active_devices(), 0);
    assert_eq!(stats.active_reservations(), 2);
    assert_eq!(stats.peak_total_usage().bytes(), 100);
    assert_eq!(stats.peak_total_usage().frames(), 3);
    assert_eq!(stats.denied_reservations(), 2);

    drop((first, second));
    let released = manager.stats().unwrap();
    assert_eq!(released.total_usage().bytes(), 0);
    assert_eq!(released.total_usage().frames(), 0);
    assert_eq!(released.active_projects(), 0);
    assert_eq!(released.active_reservations(), 0);

    let frame_manager = CacheBudgetManager::new(budgets((1_000, 2), (1_000, 2), (1_000, 2)));
    let held = frame_manager
        .reserve_host(first_project, cost(1, 2))
        .unwrap();
    let frame_error = frame_manager
        .reserve_host(second_project, cost(1, 1))
        .unwrap_err();
    assert_eq!(
        frame_error.contexts().last().unwrap().field("limit"),
        Some("total_frames")
    );
    drop(held);
}

#[test]
fn device_admission_isolated_by_device_and_charged_to_the_gpu_pool() {
    let project = ProjectId::from_raw(10);
    let other_project = ProjectId::from_raw(11);
    let first_device = DeviceId::from_raw(20);
    let second_device = DeviceId::from_raw(21);
    let manager = CacheBudgetManager::new(budgets((128, 8), (128, 8), (64, 2)));
    let first_gpu = GpuMemoryPool::new(MemoryBudget::new(64, 64).unwrap());
    let second_gpu = GpuMemoryPool::new(MemoryBudget::new(64, 64).unwrap());

    let first = manager
        .reserve_device(project, first_device, cost(40, 1), &first_gpu, &[])
        .unwrap();
    assert_eq!(first.project_id(), project);
    assert_eq!(first.device_id(), Some(first_device));
    assert_eq!(first.cost(), cost(40, 1));
    assert_eq!(
        first_gpu
            .stats()
            .unwrap()
            .resident_bytes_for(MemoryClass::Cache),
        40
    );

    let device_error = manager
        .reserve_device(other_project, first_device, cost(25, 1), &first_gpu, &[])
        .unwrap_err();
    assert_eq!(
        device_error.contexts().last().unwrap().field("limit"),
        Some("device_bytes")
    );
    assert_eq!(first_gpu.stats().unwrap().resident_bytes(), 40);

    let second = manager
        .reserve_device(other_project, second_device, cost(40, 1), &second_gpu, &[])
        .unwrap();
    let stats = manager.stats().unwrap();
    assert_eq!(stats.device_usage(first_device).bytes(), 40);
    assert_eq!(stats.device_usage(second_device).bytes(), 40);
    assert_eq!(stats.active_devices(), 2);

    drop((first, second));
    assert_eq!(first_gpu.stats().unwrap().resident_bytes(), 0);
    assert_eq!(second_gpu.stats().unwrap().resident_bytes(), 0);
    let released = manager.stats().unwrap();
    assert_eq!(released.active_projects(), 0);
    assert_eq!(released.active_devices(), 0);
}

#[test]
fn gpu_refusal_rolls_back_every_cache_scope() {
    let project = ProjectId::from_raw(30);
    let device = DeviceId::from_raw(31);
    let manager = CacheBudgetManager::new(budgets((128, 8), (128, 8), (128, 8)));
    let gpu = GpuMemoryPool::new(MemoryBudget::new(32, 32).unwrap());

    let error = manager
        .reserve_device(project, device, cost(40, 1), &gpu, &[])
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
    assert_eq!(error.recoverability(), Recoverability::Retryable);
    assert_eq!(
        error.contexts().last().unwrap().field("project"),
        Some(project.to_string().as_str())
    );
    assert_eq!(
        error.contexts().last().unwrap().field("device"),
        Some(device.to_string().as_str())
    );
    assert_eq!(gpu.stats().unwrap().resident_bytes(), 0);

    let stats = manager.stats().unwrap();
    assert_eq!(stats.total_usage().bytes(), 0);
    assert_eq!(stats.total_usage().frames(), 0);
    assert_eq!(stats.active_projects(), 0);
    assert_eq!(stats.active_devices(), 0);
    assert_eq!(stats.active_reservations(), 0);
}

#[test]
fn concurrent_admission_cannot_cross_any_hard_limit() {
    assert_send_sync::<CacheBudgetManager>();
    assert_send_sync::<CacheBudgetReservation>();

    let manager = CacheBudgetManager::new(budgets((64, 2), (64, 2), (64, 2)));
    let project = ProjectId::from_raw(40);
    let barrier = Arc::new(Barrier::new(8));
    let (sender, receiver) = mpsc::channel();
    let mut workers = Vec::new();
    for _ in 0..8 {
        let manager = manager.clone();
        let barrier = Arc::clone(&barrier);
        let sender = sender.clone();
        workers.push(std::thread::spawn(move || {
            barrier.wait();
            sender
                .send(manager.reserve_host(project, cost(32, 1)))
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
    let stats = manager.stats().unwrap();
    assert_eq!(stats.total_usage().bytes(), 64);
    assert_eq!(stats.total_usage().frames(), 2);
    assert_eq!(stats.project_usage(project).bytes(), 64);
    assert_eq!(stats.denied_reservations(), 6);

    drop(reservations);
    let released = manager.stats().unwrap();
    assert_eq!(released.total_usage().bytes(), 0);
    assert_eq!(released.active_projects(), 0);
    assert_eq!(released.active_reservations(), 0);
}
