//! GPU-resident display presentation derived from an explicit output transform.

use std::borrow::Cow;
use std::sync::Arc;

use superi_core::color_space::TransferFunction;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_gpu::resource::GpuResources;
use superi_gpu::submission::{GpuFence, GpuSubmissionQueue};
use superi_gpu::surface::ViewportFrame;
use superi_gpu::texture::GpuTexture;
use superi_gpu::wgpu;

use crate::gamut::{ChromaticAdaptation, GamutMapping, WideGamutTransform};
use crate::transform_out::{OutputColorTransform, OutputTargetKind, ToneMapping};

const COMPONENT: &str = "superi-color.gpu-display";
const SOURCE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba16Float;

/// A deterministic diagnostic interpretation of one canonical viewer result.
#[derive(Clone, Copy, Debug, Default, Eq, Hash, PartialEq)]
pub enum GpuDisplayView {
    /// Present the canonical image through the configured output transform.
    #[default]
    Image,
    /// Present straight alpha as an opaque neutral image.
    Alpha,
    /// Present unassociated scene-linear red as an opaque neutral image.
    Red,
    /// Present unassociated scene-linear green as an opaque neutral image.
    Green,
    /// Present unassociated scene-linear blue as an opaque neutral image.
    Blue,
    /// Present source-space CIE Y as an opaque neutral image.
    Luminance,
    /// Present fixed source-space exposure bands through the output transform.
    FalseColor,
    /// Present under and over range after display-linear gamut conversion.
    Clipping,
}

impl GpuDisplayView {
    /// Every supported view in stable shell and diagnostic order.
    pub const ALL: &'static [Self] = &[
        Self::Image,
        Self::Alpha,
        Self::Red,
        Self::Green,
        Self::Blue,
        Self::Luminance,
        Self::FalseColor,
        Self::Clipping,
    ];

    /// Returns the stable cross-boundary code.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Image => "image",
            Self::Alpha => "alpha",
            Self::Red => "red",
            Self::Green => "green",
            Self::Blue => "blue",
            Self::Luminance => "luminance",
            Self::FalseColor => "false_color",
            Self::Clipping => "clipping",
        }
    }

    /// Returns the linear-light stage whose values define this view.
    #[must_use]
    pub const fn analysis_stage(self) -> &'static str {
        match self {
            Self::Clipping => "display_linear",
            _ => "source_scene_linear",
        }
    }
}

/// A centered, aspect-preserving rectangle in physical target pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DisplayViewport {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl DisplayViewport {
    /// Fits one 2D source extent into one 2D target extent without a resolution cap.
    pub fn aspect_fit(source: wgpu::Extent3d, target: wgpu::Extent3d) -> Result<Self> {
        validate_extent(source, "source")?;
        validate_extent(target, "target")?;
        let scale = (f64::from(target.width) / f64::from(source.width))
            .min(f64::from(target.height) / f64::from(source.height));
        let width = f64::from(source.width) * scale;
        let height = f64::from(source.height) * scale;
        let x = (f64::from(target.width) - width) * 0.5;
        let y = (f64::from(target.height) - height) * 0.5;
        if [x, y, width, height]
            .into_iter()
            .any(|value| !value.is_finite() || value < 0.0 || value > f64::from(f32::MAX))
        {
            return Err(invalid(
                "fit_display_viewport",
                "display viewport dimensions cannot be represented by wgpu",
            ));
        }
        Ok(Self {
            x: x as f32,
            y: y as f32,
            width: width as f32,
            height: height as f32,
        })
    }

    /// Returns the physical left offset.
    #[must_use]
    pub const fn x(self) -> f32 {
        self.x
    }

    /// Returns the physical top offset.
    #[must_use]
    pub const fn y(self) -> f32 {
        self.y
    }

    /// Returns the fitted physical width.
    #[must_use]
    pub const fn width(self) -> f32 {
        self.width
    }

    /// Returns the fitted physical height.
    #[must_use]
    pub const fn height(self) -> f32 {
        self.height
    }
}

/// A render pipeline that converts one canonical working texture into a display attachment.
pub struct GpuDisplayPresenter<'device> {
    reference: OutputColorTransform,
    view: GpuDisplayView,
    target_format: wgpu::TextureFormat,
    resources: GpuResources<'device>,
    resource_scope: u64,
    identity: Arc<()>,
    bind_group_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    pipeline: wgpu::RenderPipeline,
}

impl<'device> GpuDisplayPresenter<'device> {
    /// Builds the display pipeline for one managed device and surface format.
    pub fn new(
        resources: &GpuResources<'device>,
        reference: OutputColorTransform,
        target_format: wgpu::TextureFormat,
    ) -> Result<Self> {
        Self::new_with_view(resources, reference, target_format, GpuDisplayView::Image)
    }

    /// Builds the display pipeline with one explicit diagnostic interpretation.
    pub fn new_with_view(
        resources: &GpuResources<'device>,
        reference: OutputColorTransform,
        target_format: wgpu::TextureFormat,
        view: GpuDisplayView,
    ) -> Result<Self> {
        if reference.target_kind() != OutputTargetKind::Display
            || reference.destination().transfer() != TransferFunction::Srgb
        {
            return Err(unsupported(
                "create_gpu_display_presenter",
                "the native viewport currently requires an explicit sRGB display transform",
            ));
        }
        if reference.options().tone_mapping() != ToneMapping::None {
            return Err(unsupported(
                "create_gpu_display_presenter",
                "GPU viewport tone mapping is not implemented by this presentation slice",
            ));
        }

        let device = resources.device().wgpu_device();
        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("gpu display source layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: false },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::NonFiltering),
                    count: None,
                },
            ],
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("gpu display nearest sampler"),
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("gpu display pipeline layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("gpu display transform"),
            source: wgpu::ShaderSource::Wgsl(Cow::Owned(shader_source(
                reference,
                view,
                target_format.is_srgb(),
            )?)),
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("gpu display presenter"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vertex_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fragment_main"),
                compilation_options: wgpu::PipelineCompilationOptions::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
            cache: None,
        });

        Ok(Self {
            reference,
            view,
            target_format,
            resources: resources.clone(),
            resource_scope: resources.scope_id(),
            identity: Arc::new(()),
            bind_group_layout,
            sampler,
            pipeline,
        })
    }

    /// Returns the exact CPU reference that derived the shader.
    #[must_use]
    pub const fn reference(&self) -> OutputColorTransform {
        self.reference
    }

    /// Returns the diagnostic interpretation compiled into this presenter.
    #[must_use]
    pub const fn view(&self) -> GpuDisplayView {
        self.view
    }

    /// Returns the configured display attachment format.
    #[must_use]
    pub const fn target_format(&self) -> wgpu::TextureFormat {
        self.target_format
    }

    /// Returns the canonical GPU working texture format.
    #[must_use]
    pub const fn source_format(&self) -> wgpu::TextureFormat {
        SOURCE_FORMAT
    }

    /// Retains and binds one canonical GPU-resident render result.
    pub fn prepare_source(&self, source: GpuTexture) -> Result<GpuDisplaySource> {
        validate_source(self.resource_scope, &source)?;
        let view = source.raw().create_view(&wgpu::TextureViewDescriptor {
            label: Some("gpu display source"),
            format: Some(SOURCE_FORMAT),
            dimension: Some(wgpu::TextureViewDimension::D2),
            aspect: wgpu::TextureAspect::All,
            base_mip_level: 0,
            mip_level_count: Some(1),
            base_array_layer: 0,
            array_layer_count: Some(1),
        });
        let bind_group =
            self.resources
                .device()
                .wgpu_device()
                .create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("gpu display source"),
                    layout: &self.bind_group_layout,
                    entries: &[
                        wgpu::BindGroupEntry {
                            binding: 0,
                            resource: wgpu::BindingResource::TextureView(&view),
                        },
                        wgpu::BindGroupEntry {
                            binding: 1,
                            resource: wgpu::BindingResource::Sampler(&self.sampler),
                        },
                    ],
                });
        Ok(GpuDisplaySource(Arc::new(GpuDisplaySourceInner {
            presenter_identity: Arc::clone(&self.identity),
            source,
            _view: view,
            bind_group,
        })))
    }

    /// Encodes direct presentation into an acquired or managed display texture view.
    pub fn encode(
        &self,
        source: &GpuDisplaySource,
        target: &wgpu::TextureView,
        target_extent: wgpu::Extent3d,
    ) -> Result<EncodedGpuDisplayFrame> {
        if !Arc::ptr_eq(&self.identity, &source.0.presenter_identity) {
            return Err(conflict(
                "encode_gpu_display",
                "display source belongs to a different presenter lifetime",
            ));
        }
        let viewport = DisplayViewport::aspect_fit(source.0.source.info().size(), target_extent)?;
        let mut encoder = self
            .resources
            .device()
            .wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("gpu display presentation"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("gpu display presentation"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_viewport(
                viewport.x(),
                viewport.y(),
                viewport.width(),
                viewport.height(),
                0.0,
                1.0,
            );
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &source.0.bind_group, &[]);
            pass.draw(0..3, 0..1);
        }
        Ok(EncodedGpuDisplayFrame {
            command_buffer: encoder.finish(),
            source: source.clone(),
            viewport,
        })
    }
}

struct GpuDisplaySourceInner {
    presenter_identity: Arc<()>,
    source: GpuTexture,
    _view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

/// One prepared GPU render result and all resources needed to sample it.
#[derive(Clone)]
pub struct GpuDisplaySource(Arc<GpuDisplaySourceInner>);

impl GpuDisplaySource {
    /// Returns the retained render result.
    #[must_use]
    pub fn texture(&self) -> &GpuTexture {
        &self.0.source
    }
}

/// One ordered presentation command with mandatory source retention.
#[must_use = "submit the encoded display frame through its GPU submission owner"]
pub struct EncodedGpuDisplayFrame {
    command_buffer: wgpu::CommandBuffer,
    source: GpuDisplaySource,
    viewport: DisplayViewport,
}

impl EncodedGpuDisplayFrame {
    /// Returns the aspect-preserving target viewport encoded into this frame.
    #[must_use]
    pub const fn viewport(&self) -> DisplayViewport {
        self.viewport
    }

    /// Submits an offscreen display target while retaining the sampled source.
    pub fn submit(self, submissions: &GpuSubmissionQueue<'_>) -> Result<GpuFence> {
        let mut retained = submissions.resources();
        retained.retain(self.source);
        submissions.submit([self.command_buffer], retained)
    }

    /// Submits and presents one acquired native viewport frame in order.
    pub fn submit_and_present<'surface, 'device>(
        self,
        frame: ViewportFrame<'surface, 'device>,
        submissions: &GpuSubmissionQueue<'device>,
    ) -> Result<GpuFence> {
        let mut retained = submissions.resources();
        retained.retain(self.source);
        frame.submit_and_present(submissions, [self.command_buffer], retained)
    }
}

fn validate_extent(extent: wgpu::Extent3d, role: &'static str) -> Result<()> {
    if extent.width == 0 || extent.height == 0 || extent.depth_or_array_layers != 1 {
        return Err(invalid(
            "fit_display_viewport",
            "display extents must be nonzero single-layer 2D images",
        )
        .with_context(ErrorContext::new(COMPONENT, "inspect_extent").with_field("role", role)));
    }
    Ok(())
}

fn validate_source(resource_scope: u64, source: &GpuTexture) -> Result<()> {
    let info = source.info();
    if source.id().scope() != resource_scope
        || info.dimension() != wgpu::TextureDimension::D2
        || info.format() != SOURCE_FORMAT
        || info.mip_level_count() != 1
        || info.sample_count() != 1
        || info.size().depth_or_array_layers != 1
        || !info.usage().contains(wgpu::TextureUsages::TEXTURE_BINDING)
    {
        return Err(invalid(
            "prepare_gpu_display_source",
            "display source must be a canonical managed Rgba16Float texture in this presenter lifetime",
        ));
    }
    Ok(())
}

fn shader_source(
    reference: OutputColorTransform,
    view: GpuDisplayView,
    srgb_attachment: bool,
) -> Result<String> {
    let gamut = reference.gamut_transform();
    let matrix = gamut.matrix();
    let luma = gamut.destination_luma();
    let source_primaries = reference.source().color_space().primaries();
    let source_luma = WideGamutTransform::new(
        source_primaries,
        source_primaries,
        ChromaticAdaptation::None,
        GamutMapping::Preserve,
    )?
    .destination_luma();
    let mapping = match gamut.mapping() {
        GamutMapping::Preserve => "return converted;",
        GamutMapping::ClipNegative => "return max(converted, vec3<f32>(0.0));",
        GamutMapping::PreserveLuminance => {
            "let minimum = min(converted.x, min(converted.y, converted.z));\n    if minimum >= 0.0 { return converted; }\n    let luminance = dot(DESTINATION_LUMA, converted);\n    if luminance <= 0.0 { return converted; }\n    let scale = luminance / (luminance - minimum);\n    return max(vec3<f32>(0.0), luminance + scale * (converted - luminance));"
        }
    };
    let source_analysis = match view {
        GpuDisplayView::Image | GpuDisplayView::Clipping => "return rgb;",
        GpuDisplayView::Alpha => "return vec3<f32>(alpha);",
        GpuDisplayView::Red => "return vec3<f32>(rgb.r);",
        GpuDisplayView::Green => "return vec3<f32>(rgb.g);",
        GpuDisplayView::Blue => "return vec3<f32>(rgb.b);",
        GpuDisplayView::Luminance => "return vec3<f32>(dot(SOURCE_LUMA, rgb));",
        GpuDisplayView::FalseColor => "return false_color(dot(SOURCE_LUMA, rgb));",
    };
    let display_analysis = if view == GpuDisplayView::Clipping {
        "let under = any(rgb < vec3<f32>(0.0));\n    let over = any(rgb > vec3<f32>(1.0));\n    if under && over { return vec3<f32>(1.0, 0.0, 1.0); }\n    if under { return vec3<f32>(0.0, 0.0, 1.0); }\n    if over { return vec3<f32>(1.0, 0.0, 0.0); }\n    return vec3<f32>(clamp(dot(DESTINATION_LUMA, rgb), 0.0, 1.0));"
    } else {
        "return rgb;"
    };
    let coverage = if view == GpuDisplayView::Image {
        "rgba.a"
    } else {
        "1.0"
    };
    let target_output = if srgb_attachment {
        "return vec4<f32>(srgb_decode(encoded * coverage), 1.0);"
    } else {
        "return vec4<f32>(encoded * coverage, 1.0);"
    };

    Ok(format!(
        "const ROW_0: vec3<f32> = vec3<f32>({m00}, {m01}, {m02});\n\
const ROW_1: vec3<f32> = vec3<f32>({m10}, {m11}, {m12});\n\
const ROW_2: vec3<f32> = vec3<f32>({m20}, {m21}, {m22});\n\
const DESTINATION_LUMA: vec3<f32> = vec3<f32>({l0}, {l1}, {l2});\n\
const SOURCE_LUMA: vec3<f32> = vec3<f32>({s0}, {s1}, {s2});\n\n\
@group(0) @binding(0) var source_texture: texture_2d<f32>;\n\
@group(0) @binding(1) var source_sampler: sampler;\n\n\
struct VertexOutput {{ @builtin(position) position: vec4<f32>, @location(0) uv: vec2<f32> }};\n\n\
@vertex\nfn vertex_main(@builtin(vertex_index) index: u32) -> VertexOutput {{\n\
    let positions = array<vec2<f32>, 3>(vec2<f32>(-1.0, -3.0), vec2<f32>(3.0, 1.0), vec2<f32>(-1.0, 1.0));\n\
    var output: VertexOutput;\n\
    output.position = vec4<f32>(positions[index], 0.0, 1.0);\n\
    output.uv = positions[index] * vec2<f32>(0.5, -0.5) + vec2<f32>(0.5);\n\
    return output;\n\
}}\n\n\
fn map_gamut(rgb: vec3<f32>) -> vec3<f32> {{\n\
    let converted = vec3<f32>(dot(ROW_0, rgb), dot(ROW_1, rgb), dot(ROW_2, rgb));\n\
    {mapping}\n\
}}\n\n\
fn false_color(luminance: f32) -> vec3<f32> {{\n\
    if luminance < (1.0 / 64.0) {{ return vec3<f32>(0.25, 0.0, 0.5); }}\n\
    if luminance < (1.0 / 16.0) {{ return vec3<f32>(0.0, 0.0, 1.0); }}\n\
    if luminance < (1.0 / 4.0) {{ return vec3<f32>(0.0, 1.0, 1.0); }}\n\
    if luminance < 1.0 {{ return vec3<f32>(0.0, 1.0, 0.0); }}\n\
    if luminance < 2.0 {{ return vec3<f32>(1.0, 1.0, 0.0); }}\n\
    if luminance < 4.0 {{ return vec3<f32>(1.0, 0.25, 0.0); }}\n\
    return vec3<f32>(1.0, 0.0, 0.0);\n\
}}\n\n\
fn analyze_source(rgb: vec3<f32>, alpha: f32) -> vec3<f32> {{\n\
    {source_analysis}\n\
}}\n\n\
fn analyze_display(rgb: vec3<f32>) -> vec3<f32> {{\n\
    {display_analysis}\n\
}}\n\n\
fn srgb_encode(value: vec3<f32>) -> vec3<f32> {{\n\
    let absolute = abs(value);\n\
    let curved = sign(value) * (1.055 * pow(absolute, vec3<f32>(1.0 / 2.4)) - 0.055);\n\
    return select(curved, 12.92 * value, absolute <= vec3<f32>(0.0031308));\n\
}}\n\n\
fn srgb_decode(value: vec3<f32>) -> vec3<f32> {{\n\
    let absolute = abs(value);\n\
    let curved = sign(value) * pow((absolute + 0.055) / 1.055, vec3<f32>(2.4));\n\
    return select(curved, value / 12.92, absolute <= vec3<f32>(0.04045));\n\
}}\n\n\
@fragment\nfn fragment_main(input: VertexOutput) -> @location(0) vec4<f32> {{\n\
    let rgba = textureSampleLevel(source_texture, source_sampler, input.uv, 0.0);\n\
    var straight = vec3<f32>(0.0);\n\
    if rgba.a > 0.0 {{ straight = rgba.rgb / rgba.a; }}\n\
    let analyzed = analyze_source(straight, rgba.a);\n\
    let converted = map_gamut(analyzed);\n\
    let viewed = analyze_display(converted);\n\
    let encoded = srgb_encode(viewed);\n\
    let coverage = {coverage};\n\
    {target_output}\n\
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
        s0 = literal(source_luma[0]),
        s1 = literal(source_luma[1]),
        s2 = literal(source_luma[2]),
    ))
}

fn literal(value: f64) -> String {
    format!("{:.9}", value as f32)
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn conflict(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn unsupported(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
