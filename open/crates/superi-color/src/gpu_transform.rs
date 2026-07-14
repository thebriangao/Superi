//! Managed GPU production execution for deterministic wide-gamut transforms.

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_gpu::binding::{GpuBindGroupDescriptor, GpuBindGroupEntry, GpuBindGroupLayout};
use superi_gpu::pass::{
    GpuComputePassCommand, GpuComputePassPlan, GpuPassBatch, GpuPassSubmission,
};
use superi_gpu::pipeline::{GpuComputePipeline, GpuComputePipelineDescriptor};
use superi_gpu::resource::GpuResources;
use superi_gpu::shader::{GpuShaderModuleDescriptor, ShaderCache};
use superi_gpu::submission::GpuSubmissionQueue;
use superi_gpu::texture::GpuTexture;
use superi_gpu::wgpu;

use crate::gamut::{GamutMapping, WideGamutTransform};

const COMPONENT: &str = "superi-color.gpu-transform";
const ENTRY_POINT: &str = "transform_wide_gamut";
const TEXTURE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;
const WORKGROUP_SIZE: [u32; 3] = [8, 8, 1];

/// A reference-derived compute pipeline for canonical working textures.
#[derive(Clone, Debug)]
pub struct GpuWideGamutTransform {
    reference: WideGamutTransform,
    bind_group_layout: GpuBindGroupLayout,
    pipeline: GpuComputePipeline,
}

impl GpuWideGamutTransform {
    /// Compiles the production transform for one managed device lifetime.
    pub async fn new(
        resources: &GpuResources<'_>,
        shaders: &ShaderCache<'_>,
        reference: WideGamutTransform,
    ) -> Result<Self> {
        let bind_group_layout =
            resources.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("wide gamut transform bindings"),
                entries: &[
                    wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::Texture {
                            sample_type: wgpu::TextureSampleType::Float { filterable: false },
                            view_dimension: wgpu::TextureViewDimension::D2,
                            multisampled: false,
                        },
                        count: None,
                    },
                    wgpu::BindGroupLayoutEntry {
                        binding: 1,
                        visibility: wgpu::ShaderStages::COMPUTE,
                        ty: wgpu::BindingType::StorageTexture {
                            access: wgpu::StorageTextureAccess::WriteOnly,
                            format: TEXTURE_FORMAT,
                            view_dimension: wgpu::TextureViewDimension::D2,
                        },
                        count: None,
                    },
                ],
            })?;
        let source = shader_source(reference);
        let module = shaders
            .compile_wgsl(GpuShaderModuleDescriptor {
                label: Some("wide gamut transform"),
                source: &source,
            })
            .await?;
        let pipeline_layout = resources.create_pipeline_layout(
            superi_gpu::pipeline::GpuPipelineLayoutDescriptor {
                label: Some("wide gamut transform layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            },
        )?;
        let pipeline = resources
            .create_compute_pipeline(GpuComputePipelineDescriptor {
                label: Some("wide gamut transform pipeline"),
                layout: Some(&pipeline_layout),
                module: &module,
                entry_point: ENTRY_POINT,
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                cache: None,
            })
            .await?;

        Ok(Self {
            reference,
            bind_group_layout,
            pipeline,
        })
    }

    /// Returns the exact CPU contract used to derive this pipeline.
    #[must_use]
    pub const fn reference(&self) -> WideGamutTransform {
        self.reference
    }

    /// Returns the canonical input and output texture format.
    #[must_use]
    pub const fn texture_format(&self) -> wgpu::TextureFormat {
        TEXTURE_FORMAT
    }

    /// Returns the fixed production workgroup dimensions.
    #[must_use]
    pub const fn workgroup_size(&self) -> [u32; 3] {
        WORKGROUP_SIZE
    }

    /// Encodes one GPU-resident transform without queue submission or readback.
    pub fn encode(
        &self,
        resources: &GpuResources<'_>,
        source: GpuTexture,
    ) -> Result<EncodedGpuWideGamutTransform> {
        validate_source(&source)?;
        let source_view = resources.create_texture_view(
            &source,
            &wgpu::TextureViewDescriptor {
                label: Some("wide gamut transform source"),
                format: Some(TEXTURE_FORMAT),
                dimension: Some(wgpu::TextureViewDimension::D2),
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                mip_level_count: Some(1),
                base_array_layer: 0,
                array_layer_count: Some(1),
            },
        )?;
        let extent = source.info().size();
        let output = resources.create_texture(&wgpu::TextureDescriptor {
            label: Some("wide gamut transform output"),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TEXTURE_FORMAT,
            usage: wgpu::TextureUsages::STORAGE_BINDING
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        })?;
        let output_view = resources.create_texture_view(
            &output,
            &wgpu::TextureViewDescriptor {
                label: Some("wide gamut transform output"),
                format: Some(TEXTURE_FORMAT),
                dimension: Some(wgpu::TextureViewDimension::D2),
                aspect: wgpu::TextureAspect::All,
                base_mip_level: 0,
                mip_level_count: Some(1),
                base_array_layer: 0,
                array_layer_count: Some(1),
            },
        )?;
        let bind_group = resources.create_bind_group(GpuBindGroupDescriptor {
            label: Some("wide gamut transform resources"),
            layout: &self.bind_group_layout,
            entries: &[
                GpuBindGroupEntry::texture_view(0, source_view),
                GpuBindGroupEntry::texture_view(1, output_view),
            ],
        })?;

        let mut plan = GpuComputePassPlan::new("wide gamut transform");
        plan.push_command(GpuComputePassCommand::SetPipeline(self.pipeline.clone()));
        plan.push_command(GpuComputePassCommand::SetBindGroup {
            index: 0,
            bind_group,
            dynamic_offsets: Vec::new(),
        });
        plan.push_command(GpuComputePassCommand::Dispatch {
            x: extent.width.div_ceil(WORKGROUP_SIZE[0]),
            y: extent.height.div_ceil(WORKGROUP_SIZE[1]),
            z: 1,
        });
        let mut encoder = resources.create_pass_encoder(Some("wide gamut transform"));
        encoder.encode_compute(plan)?;
        let batch = encoder.finish()?;

        Ok(EncodedGpuWideGamutTransform { output, batch })
    }
}

/// An output allocation and ordered compute batch awaiting submission.
#[derive(Debug)]
#[must_use = "submit the encoded color transform through GpuSubmissionQueue"]
pub struct EncodedGpuWideGamutTransform {
    output: GpuTexture,
    batch: GpuPassBatch,
}

impl EncodedGpuWideGamutTransform {
    /// Returns the GPU-resident output allocation.
    #[must_use]
    pub const fn output(&self) -> &GpuTexture {
        &self.output
    }

    /// Returns the ordered managed pass batch.
    #[must_use]
    pub const fn batch(&self) -> &GpuPassBatch {
        &self.batch
    }

    /// Submits the transform and returns its fence-governed output owner.
    pub fn submit(
        self,
        submissions: &GpuSubmissionQueue<'_>,
    ) -> Result<SubmittedGpuWideGamutTransform> {
        let submission = submissions.submit_pass_batch(self.batch)?;
        Ok(SubmittedGpuWideGamutTransform {
            output: self.output,
            submission,
        })
    }
}

/// A submitted production transform whose output remains GPU resident.
#[derive(Debug)]
pub struct SubmittedGpuWideGamutTransform {
    output: GpuTexture,
    submission: GpuPassSubmission,
}

impl SubmittedGpuWideGamutTransform {
    /// Returns the GPU-resident transformed texture.
    #[must_use]
    pub const fn output(&self) -> &GpuTexture {
        &self.output
    }

    /// Returns the ordered pass metadata and completion fence.
    #[must_use]
    pub const fn submission(&self) -> &GpuPassSubmission {
        &self.submission
    }

    /// Consumes the handle and returns the output plus submission metadata.
    #[must_use]
    pub fn into_parts(self) -> (GpuTexture, GpuPassSubmission) {
        (self.output, self.submission)
    }
}

fn validate_source(source: &GpuTexture) -> Result<()> {
    let info = source.info();
    let valid = info.dimension() == wgpu::TextureDimension::D2
        && info.format() == TEXTURE_FORMAT
        && info.mip_level_count() == 1
        && info.sample_count() == 1
        && info.size().depth_or_array_layers == 1
        && info.usage().contains(wgpu::TextureUsages::TEXTURE_BINDING);
    if valid {
        return Ok(());
    }
    Err(Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        "wide-gamut GPU input must be one single-sample 2D RGBA16F texture with TEXTURE_BINDING usage",
    )
    .with_context(
        ErrorContext::new(COMPONENT, "encode_wide_gamut_transform")
            .with_field("source_id", source.id().to_string())
            .with_field("format", format!("{:?}", info.format()))
            .with_field("extent", format!("{:?}", info.size()))
            .with_field("usage", format!("{:?}", info.usage())),
    ))
}

fn shader_source(reference: WideGamutTransform) -> String {
    let matrix = reference.matrix();
    let luma = reference.destination_luma();
    let mapping = match reference.mapping() {
        GamutMapping::Preserve => "return converted;",
        GamutMapping::ClipNegative => "return max(converted, vec3<f32>(0.0));",
        GamutMapping::PreserveLuminance => {
            "let minimum = min(converted.x, min(converted.y, converted.z));\n    if minimum >= 0.0 { return converted; }\n    let luminance = dot(DESTINATION_LUMA, converted);\n    if luminance <= 0.0 { return converted; }\n    let scale = luminance / (luminance - minimum);\n    return max(vec3<f32>(0.0), luminance + scale * (converted - luminance));"
        }
    };

    format!(
        "const ROW_0: vec3<f32> = vec3<f32>({m00}, {m01}, {m02});\n\
const ROW_1: vec3<f32> = vec3<f32>({m10}, {m11}, {m12});\n\
const ROW_2: vec3<f32> = vec3<f32>({m20}, {m21}, {m22});\n\
const DESTINATION_LUMA: vec3<f32> = vec3<f32>({l0}, {l1}, {l2});\n\n\
@group(0) @binding(0) var source_texture: texture_2d<f32>;\n\
@group(0) @binding(1) var output_texture: texture_storage_2d<rgba16float, write>;\n\n\
fn transform_rgb(rgb: vec3<f32>) -> vec3<f32> {{\n\
    let converted = vec3<f32>(dot(ROW_0, rgb), dot(ROW_1, rgb), dot(ROW_2, rgb));\n\
    {mapping}\n\
}}\n\n\
@compute @workgroup_size(8, 8, 1)\n\
fn {ENTRY_POINT}(@builtin(global_invocation_id) id: vec3<u32>) {{\n\
    let dimensions = textureDimensions(source_texture);\n\
    if id.x >= dimensions.x || id.y >= dimensions.y {{ return; }}\n\
    let rgba = textureLoad(source_texture, vec2<i32>(id.xy), 0);\n\
    if rgba.a == 0.0 {{\n\
        textureStore(output_texture, vec2<i32>(id.xy), vec4<f32>(0.0));\n\
        return;\n\
    }}\n\
    var rgb = rgba.rgb;\n\
    if {unassociate} {{ rgb = rgb / rgba.a; }}\n\
    rgb = transform_rgb(rgb);\n\
    if {unassociate} {{ rgb = rgb * rgba.a; }}\n\
    textureStore(output_texture, vec2<i32>(id.xy), vec4<f32>(rgb, rgba.a));\n\
}}\n",
        m00 = literal(matrix[0][0]),
        m01 = literal(matrix[0][1]),
        m02 = literal(matrix[0][2]),
        m10 = literal(matrix[1][0]),
        m11 = literal(matrix[1][1]),
        m12 = literal(matrix[1][2]),
        m20 = literal(matrix[2][0]),
        m21 = literal(matrix[2][1]),
        m22 = literal(matrix[2][2]),
        l0 = literal(luma[0]),
        l1 = literal(luma[1]),
        l2 = literal(luma[2]),
        unassociate = reference.mapping() != GamutMapping::Preserve,
    )
}

fn literal(value: f64) -> String {
    format!("{:.9}", value as f32)
}
