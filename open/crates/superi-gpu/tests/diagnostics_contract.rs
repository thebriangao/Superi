use std::num::NonZeroUsize;

use superi_core::diagnostics::FieldVisibility;
use superi_core::error::{ErrorCategory, Recoverability};
use superi_gpu::device::{
    AdapterSelection, DeviceRequest, GpuDevice, GpuInstance, InstanceOptions,
};
use superi_gpu::diagnostics::GpuTimingConfig;
use superi_gpu::pass::{
    GpuColorAttachment, GpuComputePassCommand, GpuComputePassPlan, GpuPassKind,
    GpuRenderPassCommand, GpuRenderPassPlan,
};
use superi_gpu::pipeline::{
    GpuComputePipeline, GpuComputePipelineDescriptor, GpuFragmentState,
    GpuPipelineLayoutDescriptor, GpuRenderPipeline, GpuRenderPipelineDescriptor, GpuVertexState,
};
use superi_gpu::pool::{GpuMemoryPool, MemoryBudget};
use superi_gpu::resource::{GpuResourceKind, GpuResources};
use superi_gpu::shader::{GpuShaderModuleDescriptor, ShaderCache};
use superi_gpu::submission::GpuSubmissionQueue;
use superi_gpu::wgpu;

const PRIVATE_MEDIA: &str = "/Users/editor/Unreleased Film/secret-shot-0042.exr";

fn test_device(features: wgpu::Features) -> Option<GpuDevice> {
    let instance = GpuInstance::new(InstanceOptions::default()).ok()?;
    let selection = AdapterSelection::default().with_required_features(features);
    let adapter = instance.enumerate_adapters().select(&selection).ok()?;
    pollster::block_on(
        adapter.create_device(
            &DeviceRequest::default()
                .with_label(PRIVATE_MEDIA)
                .with_required_features(features),
        ),
    )
    .ok()
}

fn compute_pipeline(resources: &GpuResources<'_>) -> GpuComputePipeline {
    let layout = resources
        .create_pipeline_layout(GpuPipelineLayoutDescriptor {
            label: Some(PRIVATE_MEDIA),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        })
        .unwrap();
    let cache = ShaderCache::new(resources, NonZeroUsize::new(1).unwrap());
    let shader = pollster::block_on(cache.compile_wgsl(GpuShaderModuleDescriptor {
        label: Some(PRIVATE_MEDIA),
        source: "@compute @workgroup_size(1) fn main() {}",
    }))
    .unwrap();
    pollster::block_on(
        resources.create_compute_pipeline(GpuComputePipelineDescriptor {
            label: Some(PRIVATE_MEDIA),
            layout: Some(&layout),
            module: &shader,
            entry_point: "main",
            compilation_options: wgpu::PipelineCompilationOptions::default(),
            cache: None,
        }),
    )
    .unwrap()
}

fn render_pipeline(resources: &GpuResources<'_>) -> GpuRenderPipeline {
    let layout = resources
        .create_pipeline_layout(GpuPipelineLayoutDescriptor {
            label: Some(PRIVATE_MEDIA),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        })
        .unwrap();
    let cache = ShaderCache::new(resources, NonZeroUsize::new(1).unwrap());
    let shader = pollster::block_on(cache.compile_wgsl(GpuShaderModuleDescriptor {
        label: Some(PRIVATE_MEDIA),
        source: r#"
@vertex
fn vertex_main(@builtin(vertex_index) index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    return vec4<f32>(positions[index], 0.0, 1.0);
}

@fragment
fn fragment_main() -> @location(0) vec4<f32> {
    return vec4<f32>(0.25, 0.5, 0.75, 1.0);
}
"#,
    }))
    .unwrap();
    let targets = [Some(wgpu::ColorTargetState {
        format: wgpu::TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: wgpu::ColorWrites::ALL,
    })];
    pollster::block_on(
        resources.create_render_pipeline(GpuRenderPipelineDescriptor {
            label: Some(PRIVATE_MEDIA),
            layout: Some(&layout),
            vertex: GpuVertexState {
                module: &shader,
                entry_point: "vertex_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(GpuFragmentState {
                module: &shader,
                entry_point: "fragment_main",
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &targets,
            }),
            multiview: None,
            cache: None,
        }),
    )
    .unwrap()
}

fn render_target(resources: &GpuResources<'_>) -> superi_gpu::texture::GpuTextureView {
    let texture = resources
        .create_texture(&wgpu::TextureDescriptor {
            label: Some(PRIVATE_MEDIA),
            size: wgpu::Extent3d {
                width: 4,
                height: 4,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })
        .unwrap();
    resources
        .create_texture_view(&texture, &wgpu::TextureViewDescriptor::default())
        .unwrap()
}

#[test]
fn aggregate_snapshot_is_device_scoped_and_user_safe() {
    let Some(device) = test_device(wgpu::Features::empty()) else {
        eprintln!("no wgpu adapter is available, skipping GPU diagnostic snapshot contract");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let submissions = GpuSubmissionQueue::new(&device).unwrap();
    let memory = GpuMemoryPool::new(MemoryBudget::new(1_024, 2_048).unwrap());

    let snapshot = resources
        .diagnostic_snapshot(&submissions, Some(&memory))
        .unwrap();
    assert_eq!(snapshot.backend(), device.adapter().info().backend);
    assert_eq!(snapshot.device_type(), device.adapter().info().device_type);
    assert!(!snapshot.timestamp_queries_enabled());
    assert_eq!(snapshot.resources().total(), resources.stats().total());
    assert_eq!(snapshot.submissions(), submissions.progress());
    assert_eq!(
        snapshot.memory().unwrap().budget(),
        MemoryBudget::new(1_024, 2_048).unwrap()
    );

    let event = snapshot.user_safe_event().unwrap();
    assert_eq!(event.name(), "gpu.snapshot");
    assert_eq!(event.component(), "superi-gpu.diagnostics");
    assert!(event
        .fields()
        .values()
        .all(|field| field.visibility() == FieldVisibility::UserSafe));
    for output in [format!("{snapshot:?}"), format!("{event:?}")] {
        assert!(!output.contains(PRIVATE_MEDIA));
        assert!(!output.contains("secret-shot"));
    }

    let Some(other_device) = test_device(wgpu::Features::empty()) else {
        eprintln!("a second wgpu device is unavailable, skipping diagnostic lifetime contract");
        return;
    };
    let other_submissions = GpuSubmissionQueue::new(&other_device).unwrap();
    let error = resources
        .diagnostic_snapshot(&other_submissions, None)
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Conflict);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
}

#[test]
fn timed_encoder_requires_the_optional_feature_and_enforces_capacity() {
    assert!(GpuTimingConfig::new(0).is_err());

    let Some(default_device) = test_device(wgpu::Features::empty()) else {
        eprintln!("no wgpu adapter is available, skipping GPU timing capability contract");
        return;
    };
    let default_resources = GpuResources::new(&default_device).unwrap();
    let error = default_resources
        .create_timed_pass_encoder(None, GpuTimingConfig::new(1).unwrap())
        .unwrap_err();
    assert_eq!(error.category(), ErrorCategory::Unsupported);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);

    let Some(device) = test_device(wgpu::Features::TIMESTAMP_QUERY) else {
        eprintln!(
            "active wgpu adapter has no timestamp queries, skipping timing capacity contract"
        );
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let pipeline = compute_pipeline(&resources);
    let mut encoder = resources
        .create_timed_pass_encoder(Some(PRIVATE_MEDIA), GpuTimingConfig::new(1).unwrap())
        .unwrap();
    let mut first = GpuComputePassPlan::new(PRIVATE_MEDIA);
    first.push_command(GpuComputePassCommand::SetPipeline(pipeline.clone()));
    first.push_command(GpuComputePassCommand::Dispatch { x: 1, y: 1, z: 1 });
    encoder.encode_compute(first).unwrap();

    let mut overflow = GpuComputePassPlan::new(PRIVATE_MEDIA);
    overflow.push_command(GpuComputePassCommand::SetPipeline(pipeline));
    overflow.push_command(GpuComputePassCommand::Dispatch { x: 1, y: 1, z: 1 });
    let error = encoder.encode_compute(overflow).unwrap_err();
    assert_eq!(error.category(), ErrorCategory::ResourceExhausted);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(encoder.pass_count(), 1);
}

#[test]
fn real_managed_compute_and_render_passes_return_privacy_safe_gpu_timings() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<superi_gpu::diagnostics::GpuTimingHandle>();

    let Some(device) = test_device(wgpu::Features::TIMESTAMP_QUERY) else {
        eprintln!(
            "active wgpu adapter has no timestamp queries, skipping real GPU timing contract"
        );
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let submissions = GpuSubmissionQueue::new(&device).unwrap();
    let compute_pipeline = compute_pipeline(&resources);
    let render_pipeline = render_pipeline(&resources);
    let target = render_target(&resources);
    let mut encoder = resources
        .create_timed_pass_encoder(Some(PRIVATE_MEDIA), GpuTimingConfig::new(2).unwrap())
        .unwrap();

    let mut compute = GpuComputePassPlan::new(PRIVATE_MEDIA);
    compute.push_command(GpuComputePassCommand::SetPipeline(compute_pipeline));
    compute.push_command(GpuComputePassCommand::Dispatch { x: 1, y: 1, z: 1 });
    encoder.encode_compute(compute).unwrap();

    let mut render = GpuRenderPassPlan::new(PRIVATE_MEDIA);
    render.push_color_attachment(GpuColorAttachment::new(
        target,
        wgpu::Operations {
            load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
            store: wgpu::StoreOp::Store,
        },
    ));
    render.push_command(GpuRenderPassCommand::SetPipeline(render_pipeline));
    render.push_command(GpuRenderPassCommand::Draw {
        vertices: 0..3,
        instances: 0..1,
    });
    encoder.encode_render(render).unwrap();

    let submission = submissions
        .submit_pass_batch(encoder.finish().unwrap())
        .unwrap();
    let timing = submission
        .timing()
        .expect("timed batch has a handle")
        .clone();
    assert_eq!(timing.fence().value(), submission.fence().value());
    assert!(!format!("{timing:?}").contains(PRIVATE_MEDIA));

    let report = timing.wait(&submissions).unwrap();
    assert_eq!(report.passes().len(), 2);
    assert_eq!(report.passes()[0].sequence(), 0);
    assert_eq!(report.passes()[0].kind(), GpuPassKind::Compute);
    assert_eq!(report.passes()[1].sequence(), 1);
    assert_eq!(report.passes()[1].kind(), GpuPassKind::Render);
    assert_eq!(
        report.total_nanoseconds(),
        report
            .passes()
            .iter()
            .map(|pass| pass.duration_nanoseconds())
            .fold(0_u64, u64::saturating_add)
    );

    let event = report.user_safe_event().unwrap();
    assert_eq!(event.name(), "gpu.timing.completed");
    assert!(event
        .fields()
        .values()
        .all(|field| field.visibility() == FieldVisibility::UserSafe));
    for output in [format!("{report:?}"), format!("{event:?}")] {
        assert!(!output.contains(PRIVATE_MEDIA));
        assert!(!output.contains("secret-shot"));
    }

    if let Some(other_device) = test_device(wgpu::Features::TIMESTAMP_QUERY) {
        let other_submissions = GpuSubmissionQueue::new(&other_device).unwrap();
        let error = timing.poll(&other_submissions).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::Conflict);
    }
}

#[test]
fn dropping_an_unread_timing_handle_cancels_mapping_and_retires_resources() {
    let Some(device) = test_device(wgpu::Features::TIMESTAMP_QUERY) else {
        eprintln!("active wgpu adapter has no timestamp queries, skipping timing drop contract");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let pipeline = compute_pipeline(&resources);
    let baseline_buffers = resources.stats().count(GpuResourceKind::Buffer);
    let submissions = GpuSubmissionQueue::new(&device).unwrap();
    let mut encoder = resources
        .create_timed_pass_encoder(Some(PRIVATE_MEDIA), GpuTimingConfig::new(1).unwrap())
        .unwrap();
    assert_eq!(
        resources.stats().count(GpuResourceKind::Buffer),
        baseline_buffers + 2
    );

    let mut compute = GpuComputePassPlan::new(PRIVATE_MEDIA);
    compute.push_command(GpuComputePassCommand::SetPipeline(pipeline));
    compute.push_command(GpuComputePassCommand::Dispatch { x: 1, y: 1, z: 1 });
    encoder.encode_compute(compute).unwrap();
    let submission = submissions
        .submit_pass_batch(encoder.finish().unwrap())
        .unwrap();
    let fence = submission.fence().clone();
    drop(submission);

    submissions.wait(&fence).unwrap();
    assert_eq!(
        resources.stats().count(GpuResourceKind::Buffer),
        baseline_buffers
    );
}
