use std::num::NonZeroUsize;
use std::sync::{Arc, Barrier};

use superi_core::error::ErrorCategory;
use superi_gpu::device::{
    AdapterSelection, DeviceRequest, GpuDevice, GpuInstance, InstanceOptions,
};
use superi_gpu::pipeline::{
    GpuComputePipelineDescriptor, GpuFragmentState, GpuPipelineLayoutDescriptor,
    GpuRenderPipelineDescriptor, GpuVertexState,
};
use superi_gpu::resource::{GpuResourceKind, GpuResources};
use superi_gpu::shader::{GpuShaderModuleDescriptor, ShaderBindingKind, ShaderCache, ShaderStage};

fn assert_send_sync<T: Send + Sync>() {}

fn test_device() -> Option<GpuDevice> {
    let instance = GpuInstance::new(InstanceOptions::default()).ok()?;
    let adapter = instance
        .enumerate_adapters()
        .select(&AdapterSelection::default())
        .ok()?;
    pollster::block_on(
        adapter.create_device(&DeviceRequest::default().with_label("shader contract")),
    )
    .ok()
}

const REFLECTED_SHADER: &str = r#"
override gain: f32 = 1.0;

@group(0) @binding(0)
var<storage, read_write> output: array<f32>;

@compute @workgroup_size(8, 4, 1)
fn compute_main() {
    output[0] = gain;
}

@vertex
fn vertex_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    let positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0)
    );
    return vec4<f32>(positions[vertex_index], 0.0, 1.0);
}

@fragment
fn fragment_main() -> @location(0) vec4<f32> {
    return vec4<f32>(0.25, 0.5, 0.75, 1.0);
}
"#;

#[test]
fn wgsl_compilation_reflects_entry_points_bindings_and_overrides_then_reuses_cache() {
    assert_send_sync::<ShaderCache<'static>>();
    assert_send_sync::<superi_gpu::shader::GpuShaderModule>();
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping shader contract");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let cache = ShaderCache::new(&resources, NonZeroUsize::new(4).unwrap());
    let descriptor = GpuShaderModuleDescriptor {
        label: Some("reflected shader"),
        source: REFLECTED_SHADER,
    };

    let first = pollster::block_on(cache.compile_wgsl(descriptor)).unwrap();
    let second = pollster::block_on(cache.compile_wgsl(descriptor)).unwrap();

    assert_eq!(first.id(), second.id());
    assert_eq!(resources.stats().count(GpuResourceKind::ShaderModule), 1);
    assert_eq!(first.diagnostics(), &[]);
    let reflection = first.reflection();
    let compute = reflection
        .entry_point("compute_main", ShaderStage::Compute)
        .unwrap();
    assert_eq!(compute.workgroup_size(), Some([8, 4, 1]));
    assert!(reflection
        .entry_point("vertex_main", ShaderStage::Vertex)
        .is_some());
    assert!(reflection
        .entry_point("fragment_main", ShaderStage::Fragment)
        .is_some());
    let binding = reflection.binding(0, 0).unwrap();
    assert_eq!(binding.name(), Some("output"));
    assert_eq!(
        binding.kind(),
        ShaderBindingKind::StorageBuffer { read_only: false }
    );
    assert_eq!(
        binding.visibility(),
        superi_gpu::wgpu::ShaderStages::COMPUTE
    );
    let shader_override = reflection.override_named("gain").unwrap();
    assert!(shader_override.has_default());

    let stats = cache.stats();
    assert_eq!(stats.entries(), 1);
    assert_eq!(stats.hits(), 1);
    assert_eq!(stats.misses(), 1);
    assert_eq!(stats.evictions(), 0);

    drop(first);
    drop(second);
    assert_eq!(resources.stats().count(GpuResourceKind::ShaderModule), 1);
    cache.clear();
    assert_eq!(resources.stats().count(GpuResourceKind::ShaderModule), 0);
}

#[test]
fn invalid_wgsl_is_actionable_and_failed_compilations_are_not_cached() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping shader validation contract");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let cache = ShaderCache::new(&resources, NonZeroUsize::new(2).unwrap());

    let parse_error = pollster::block_on(cache.compile_wgsl(GpuShaderModuleDescriptor {
        label: Some("invalid parse"),
        source: "@compute fn broken(",
    }))
    .unwrap_err();
    assert_eq!(parse_error.category(), ErrorCategory::InvalidInput);
    let parse_context = parse_error.contexts().last().unwrap();
    assert_eq!(parse_context.operation(), "parse_wgsl");
    assert_eq!(parse_context.field("label"), Some("invalid parse"));
    assert!(parse_context.field("line").is_some());
    assert!(parse_context.field("source_sha256").is_some());

    let device_error = pollster::block_on(cache.compile_wgsl(GpuShaderModuleDescriptor {
        label: Some("invalid device limits"),
        source: r#"
@group(999) @binding(0) var<storage, read_write> output: array<u32>;
@compute @workgroup_size(1)
fn main() { output[0] = 1u; }
"#,
    }))
    .unwrap_err();
    assert_eq!(device_error.category(), ErrorCategory::InvalidInput);
    assert_eq!(
        device_error.contexts().last().unwrap().operation(),
        "compile_wgsl"
    );

    let stats = cache.stats();
    assert_eq!(stats.entries(), 0);
    assert_eq!(stats.hits(), 0);
    assert_eq!(stats.misses(), 2);
    assert_eq!(resources.stats().count(GpuResourceKind::ShaderModule), 0);
}

#[test]
fn bounded_cache_evicts_the_least_recently_used_module_without_shortening_handles() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping shader cache contract");
        return;
    };
    let resources = GpuResources::new(&device).unwrap();
    let cache = ShaderCache::new(&resources, NonZeroUsize::new(2).unwrap());
    let source = |value| {
        format!("@compute @workgroup_size(1) fn main() {{ let value = {value}u; _ = value; }}")
    };
    let first_source = source(1);
    let second_source = source(2);
    let third_source = source(3);

    let first = pollster::block_on(cache.compile_wgsl(GpuShaderModuleDescriptor {
        label: Some("first"),
        source: &first_source,
    }))
    .unwrap();
    let second = pollster::block_on(cache.compile_wgsl(GpuShaderModuleDescriptor {
        label: Some("second"),
        source: &second_source,
    }))
    .unwrap();
    let first_hit = pollster::block_on(cache.compile_wgsl(GpuShaderModuleDescriptor {
        label: Some("first"),
        source: &first_source,
    }))
    .unwrap();
    assert_eq!(first.id(), first_hit.id());
    let third = pollster::block_on(cache.compile_wgsl(GpuShaderModuleDescriptor {
        label: Some("third"),
        source: &third_source,
    }))
    .unwrap();
    assert_eq!(cache.stats().entries(), 2);
    assert_eq!(cache.stats().hits(), 1);
    assert_eq!(cache.stats().misses(), 3);
    assert_eq!(cache.stats().evictions(), 1);
    assert_eq!(resources.stats().count(GpuResourceKind::ShaderModule), 3);

    let second_recompiled = pollster::block_on(cache.compile_wgsl(GpuShaderModuleDescriptor {
        label: Some("second"),
        source: &second_source,
    }))
    .unwrap();
    assert_ne!(second.id(), second_recompiled.id());
    assert_eq!(cache.stats().misses(), 4);
    assert_eq!(cache.stats().evictions(), 2);

    drop(first);
    drop(first_hit);
    drop(second);
    drop(third);
    drop(second_recompiled);
    cache.clear();
    assert_eq!(resources.stats().count(GpuResourceKind::ShaderModule), 0);
}

#[test]
fn managed_pipeline_compilation_validates_stages_layouts_and_device_lifetimes() {
    let Some(first_device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping shader pipeline contract");
        return;
    };
    let first_resources = GpuResources::new(&first_device).unwrap();
    let cache = ShaderCache::new(&first_resources, NonZeroUsize::new(2).unwrap());
    let shader = pollster::block_on(cache.compile_wgsl(GpuShaderModuleDescriptor {
        label: Some("managed stages"),
        source: REFLECTED_SHADER,
    }))
    .unwrap();

    let wrong_stage = pollster::block_on(first_resources.create_compute_pipeline(
        GpuComputePipelineDescriptor {
            label: Some("wrong stage"),
            layout: None,
            module: &shader,
            entry_point: "vertex_main",
            compilation_options: superi_gpu::wgpu::PipelineCompilationOptions::default(),
            cache: None,
        },
    ))
    .unwrap_err();
    assert_eq!(wrong_stage.category(), ErrorCategory::InvalidInput);
    assert_eq!(
        wrong_stage.contexts().last().unwrap().operation(),
        "create_compute_pipeline"
    );
    assert_eq!(
        first_resources
            .stats()
            .count(GpuResourceKind::ComputePipeline),
        0
    );

    let empty_layout = first_resources
        .create_pipeline_layout(GpuPipelineLayoutDescriptor {
            label: Some("empty layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        })
        .unwrap();
    let layout_error = pollster::block_on(first_resources.create_compute_pipeline(
        GpuComputePipelineDescriptor {
            label: Some("layout mismatch"),
            layout: Some(&empty_layout),
            module: &shader,
            entry_point: "compute_main",
            compilation_options: superi_gpu::wgpu::PipelineCompilationOptions::default(),
            cache: None,
        },
    ))
    .unwrap_err();
    assert_eq!(layout_error.category(), ErrorCategory::InvalidInput);
    assert_eq!(
        layout_error.contexts().last().unwrap().operation(),
        "create_compute_pipeline"
    );

    let compute = pollster::block_on(first_resources.create_compute_pipeline(
        GpuComputePipelineDescriptor {
            label: Some("managed compute"),
            layout: None,
            module: &shader,
            entry_point: "compute_main",
            compilation_options: superi_gpu::wgpu::PipelineCompilationOptions::default(),
            cache: None,
        },
    ))
    .unwrap();
    assert_eq!(compute.module().id(), shader.id());
    assert_eq!(compute.entry_point(), "compute_main");

    let render_layout = first_resources
        .create_pipeline_layout(GpuPipelineLayoutDescriptor {
            label: Some("render layout"),
            bind_group_layouts: &[],
            push_constant_ranges: &[],
        })
        .unwrap();
    let color_targets = [Some(superi_gpu::wgpu::ColorTargetState {
        format: superi_gpu::wgpu::TextureFormat::Rgba8Unorm,
        blend: None,
        write_mask: superi_gpu::wgpu::ColorWrites::ALL,
    })];
    let render = pollster::block_on(first_resources.create_render_pipeline(
        GpuRenderPipelineDescriptor {
            label: Some("managed render"),
            layout: Some(&render_layout),
            vertex: GpuVertexState {
                module: &shader,
                entry_point: "vertex_main",
                compilation_options: superi_gpu::wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: superi_gpu::wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: superi_gpu::wgpu::MultisampleState::default(),
            fragment: Some(GpuFragmentState {
                module: &shader,
                entry_point: "fragment_main",
                compilation_options: superi_gpu::wgpu::PipelineCompilationOptions::default(),
                targets: &color_targets,
            }),
            multiview: None,
            cache: None,
        },
    ))
    .unwrap();
    assert_eq!(render.vertex_module().id(), shader.id());
    assert_eq!(render.fragment_module().unwrap().id(), shader.id());

    let Some(second_device) = test_device() else {
        eprintln!("a second wgpu device is unavailable, skipping recovery ownership check");
        return;
    };
    let second_resources = GpuResources::new(&second_device).unwrap();
    let recovery_error = pollster::block_on(second_resources.create_compute_pipeline(
        GpuComputePipelineDescriptor {
            label: Some("recovered device"),
            layout: None,
            module: &shader,
            entry_point: "compute_main",
            compilation_options: superi_gpu::wgpu::PipelineCompilationOptions::default(),
            cache: None,
        },
    ))
    .unwrap_err();
    assert_eq!(recovery_error.category(), ErrorCategory::Conflict);

    drop(shader);
    cache.clear();
    assert_eq!(
        first_resources.stats().count(GpuResourceKind::ShaderModule),
        1
    );
    drop(compute);
    drop(render);
    assert_eq!(
        first_resources.stats().count(GpuResourceKind::ShaderModule),
        0
    );
}

#[test]
fn concurrent_managed_compilations_keep_device_error_scopes_ordered() {
    let Some(device) = test_device() else {
        eprintln!("no wgpu adapter is available, skipping concurrent shader contract");
        return;
    };
    let barrier = Arc::new(Barrier::new(4));

    std::thread::scope(|scope| {
        for worker in 0..4 {
            let barrier = Arc::clone(&barrier);
            let device = &device;
            scope.spawn(move || {
                let resources = GpuResources::new(device).unwrap();
                let cache = ShaderCache::new(&resources, NonZeroUsize::new(1).unwrap());
                let should_succeed = worker % 2 == 0;
                barrier.wait();
                for iteration in 0..8 {
                    let source = if should_succeed {
                        format!(
                            "const VALUE: u32 = {iteration}u; @compute @workgroup_size(1) fn main() {{ let value = VALUE; _ = value; }}"
                        )
                    } else {
                        format!(
                            "@group(999) @binding(0) var<storage, read_write> output: array<u32>; @compute @workgroup_size(1) fn main() {{ output[0] = {iteration}u; }}"
                        )
                    };
                    let result = pollster::block_on(cache.compile_wgsl(
                        GpuShaderModuleDescriptor {
                            label: Some("concurrent shader"),
                            source: &source,
                        },
                    ));
                    if should_succeed {
                        result.expect("valid concurrent compilation must stay valid");
                    } else {
                        assert_eq!(result.unwrap_err().category(), ErrorCategory::InvalidInput);
                    }
                }
            });
        }
    });
}
