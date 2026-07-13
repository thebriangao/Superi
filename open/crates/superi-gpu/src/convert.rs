//! GPU-resident pixel storage conversion.
//!
//! Conversion changes pixel packing, component representation, YUV matrix and
//! range, and alpha association. It deliberately does not change color
//! primaries or transfer functions. Those transforms belong to `superi-color`.

use std::fmt::Write as _;
use std::num::NonZeroUsize;

use superi_core::color_space::{ColorRange, ColorSpace, MatrixCoefficients};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::{
    AlphaMode, ChromaSubsampling, PixelFormat, PixelModel, PixelNumeric, PixelPacking,
};

use crate::binding::{GpuBindGroup, GpuBindGroupDescriptor, GpuBindGroupEntry, GpuBindGroupLayout};
use crate::pipeline::{
    GpuFragmentState, GpuPipelineLayoutDescriptor, GpuRenderPipeline, GpuRenderPipelineDescriptor,
    GpuVertexState,
};
use crate::resource::GpuResources;
use crate::shader::{GpuShaderModuleDescriptor, ShaderCache};
use crate::texture::GpuTextureView;
use crate::upload::UploadedFrame;

const COMPONENT: &str = "superi-gpu.convert";

/// The location of subsampled chroma samples relative to luma samples.
///
/// A location is mandatory for 4:2:0 and 4:2:2 frames because guessing it can
/// shift color boundaries. Full-resolution 4:4:4 and RGB frames do not use it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ChromaLocation {
    /// Chroma is horizontally cosited with the left luma sample and vertically centered.
    Left,
    /// Chroma is centered between the luma samples it represents.
    Center,
    /// Chroma is cosited with the top-left luma sample of each sample block.
    TopLeft,
}

/// The portable physical texture contract for one logical pixel plane.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuPlaneLayout {
    width: u32,
    height: u32,
    texture_width: u32,
    texture_height: u32,
    texture_format: wgpu::TextureFormat,
    valid_bits: u8,
    stored_bit_shift: u8,
}

impl GpuPlaneLayout {
    /// Returns the logical plane width in texels.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }

    /// Returns the logical plane height in texels.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }

    /// Returns the physical texture width used by the byte-exact upload layout.
    #[must_use]
    pub const fn texture_width(self) -> u32 {
        self.texture_width
    }

    /// Returns the physical texture height used by the byte-exact upload layout.
    #[must_use]
    pub const fn texture_height(self) -> u32 {
        self.texture_height
    }

    /// Returns the portable wgpu texture format used for this plane.
    #[must_use]
    pub const fn texture_format(self) -> wgpu::TextureFormat {
        self.texture_format
    }

    /// Returns meaningful source bits in each stored component.
    #[must_use]
    pub const fn valid_bits(self) -> u8 {
        self.valid_bits
    }

    /// Returns the number of low zero bits below each meaningful component.
    ///
    /// P010 uses a shift of six. Other current logical formats use zero.
    #[must_use]
    pub const fn stored_bit_shift(self) -> u8 {
        self.stored_bit_shift
    }

    fn sample_type(self) -> wgpu::TextureSampleType {
        match self.texture_format.sample_type(None, None) {
            Some(wgpu::TextureSampleType::Float { .. }) => {
                wgpu::TextureSampleType::Float { filterable: false }
            }
            Some(sample_type) => sample_type,
            None => unreachable!("conversion plane formats are always sampleable"),
        }
    }
}

/// Exact logical and physical representation of one GPU-resident frame.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuFrameDescriptor {
    width: u32,
    height: u32,
    pixel_format: PixelFormat,
    color_space: ColorSpace,
    alpha_mode: AlphaMode,
    chroma_location: Option<ChromaLocation>,
    plane_layouts: Vec<GpuPlaneLayout>,
}

impl GpuFrameDescriptor {
    /// Creates a frame descriptor and resolves its portable physical planes.
    pub fn new(
        width: u32,
        height: u32,
        pixel_format: PixelFormat,
        color_space: ColorSpace,
        alpha_mode: AlphaMode,
        chroma_location: Option<ChromaLocation>,
    ) -> Result<Self> {
        if width == 0 || height == 0 {
            return Err(invalid(
                "create_frame_descriptor",
                "GPU frame dimensions must be greater than zero",
            ));
        }
        if !pixel_format.has_alpha() && alpha_mode != AlphaMode::Opaque {
            return Err(invalid(
                "create_frame_descriptor",
                "pixel formats without alpha must use opaque alpha mode",
            ));
        }

        let subsampling = pixel_format.chroma_subsampling();
        match subsampling {
            Some(ChromaSubsampling::Cs420 | ChromaSubsampling::Cs422) => {
                if chroma_location.is_none() {
                    return Err(invalid(
                        "create_frame_descriptor",
                        "subsampled YUV requires an explicit chroma location",
                    ));
                }
            }
            Some(ChromaSubsampling::Cs444) | None => {
                if chroma_location.is_some() {
                    return Err(invalid(
                        "create_frame_descriptor",
                        "chroma location is only valid for subsampled YUV",
                    ));
                }
            }
            _ => {
                return Err(unsupported(
                    "create_frame_descriptor",
                    "the chroma subsampling is not supported by this build",
                ));
            }
        }

        let plane_layouts = plane_layouts(width, height, pixel_format)?;
        Ok(Self {
            width,
            height,
            pixel_format,
            color_space,
            alpha_mode,
            chroma_location,
            plane_layouts,
        })
    }

    /// Returns the exact logical frame width.
    #[must_use]
    pub const fn width(&self) -> u32 {
        self.width
    }

    /// Returns the exact logical frame height.
    #[must_use]
    pub const fn height(&self) -> u32 {
        self.height
    }

    /// Returns the logical pixel format.
    #[must_use]
    pub const fn pixel_format(&self) -> PixelFormat {
        self.pixel_format
    }

    /// Returns the color interpretation before or after storage conversion.
    #[must_use]
    pub const fn color_space(&self) -> ColorSpace {
        self.color_space
    }

    /// Returns the logical alpha association.
    #[must_use]
    pub const fn alpha_mode(&self) -> AlphaMode {
        self.alpha_mode
    }

    /// Returns the explicit location used by subsampled chroma.
    #[must_use]
    pub const fn chroma_location(&self) -> Option<ChromaLocation> {
        self.chroma_location
    }

    /// Returns physical planes in logical component order.
    #[must_use]
    pub fn plane_layouts(&self) -> &[GpuPlaneLayout] {
        &self.plane_layouts
    }
}

/// One immutable conversion between equal-sized GPU frame representations.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuConversionPlan {
    source: GpuFrameDescriptor,
    destination: GpuFrameDescriptor,
}

/// Managed texture views containing every physical plane of one frame.
#[derive(Clone, Debug)]
pub struct GpuPixelFrame<'device> {
    descriptor: GpuFrameDescriptor,
    planes: Vec<GpuTextureView>,
    retained_upload: Option<UploadedFrame<'device>>,
}

impl<'device> GpuPixelFrame<'device> {
    /// Validates exact plane count, format, geometry, mip range, and layer range.
    pub fn new(descriptor: GpuFrameDescriptor, planes: Vec<GpuTextureView>) -> Result<Self> {
        if planes.len() != descriptor.plane_layouts.len() {
            return Err(invalid(
                "create_pixel_frame",
                "managed plane count does not match the logical pixel format",
            ));
        }
        for (index, (view, layout)) in planes
            .iter()
            .zip(descriptor.plane_layouts.iter())
            .enumerate()
        {
            validate_frame_plane(index, view, *layout)?;
        }
        Ok(Self {
            descriptor,
            planes,
            retained_upload: None,
        })
    }

    /// Creates views over an uploaded frame without copying or repacking pixels.
    ///
    /// The uploaded frame is retained until after every view drops so its pooled
    /// allocations remain checked out for the complete conversion lifetime.
    pub fn from_uploaded(
        resources: &GpuResources<'device>,
        upload: UploadedFrame<'device>,
        color_space: ColorSpace,
        alpha_mode: AlphaMode,
        chroma_location: Option<ChromaLocation>,
    ) -> Result<Self> {
        let descriptor = GpuFrameDescriptor::new(
            upload.width(),
            upload.height(),
            upload.pixel_format(),
            color_space,
            alpha_mode,
            chroma_location,
        )?;
        if upload.planes().len() != descriptor.plane_layouts.len() {
            return Err(invalid(
                "adapt_uploaded_frame",
                "uploaded plane count does not match the conversion descriptor",
            ));
        }
        for (index, (uploaded, layout)) in upload
            .planes()
            .iter()
            .zip(descriptor.plane_layouts.iter())
            .enumerate()
        {
            let source = uploaded.source_size();
            let texture = uploaded.texture_size();
            if source.width != layout.width
                || source.height != layout.height
                || texture.width != layout.texture_width
                || texture.height != layout.texture_height
                || uploaded.texture_format() != layout.texture_format
            {
                return Err(invalid(
                    "adapt_uploaded_frame",
                    format!("uploaded plane {index} does not match the conversion layout"),
                ));
            }
        }
        let views = upload
            .planes()
            .iter()
            .enumerate()
            .map(|(index, plane)| {
                let label = format!("uploaded conversion plane {index}");
                resources.create_texture_view(
                    plane.texture(),
                    &wgpu::TextureViewDescriptor {
                        label: Some(&label),
                        ..Default::default()
                    },
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let mut frame = Self::new(descriptor, views)?;
        frame.retained_upload = Some(upload);
        Ok(frame)
    }

    /// Returns the immutable logical and physical frame description.
    #[must_use]
    pub const fn descriptor(&self) -> &GpuFrameDescriptor {
        &self.descriptor
    }

    /// Returns managed plane views in logical component order.
    #[must_use]
    pub fn planes(&self) -> &[GpuTextureView] {
        &self.planes
    }

    /// Returns the zero-copy upload owner retained by this frame, when present.
    #[must_use]
    pub const fn retained_upload(&self) -> Option<&UploadedFrame<'device>> {
        self.retained_upload.as_ref()
    }
}

/// Resources retained by one encoded conversion until submission ownership takes over.
#[derive(Debug)]
#[must_use = "retain the conversion lease until its command buffer has been submitted"]
pub struct GpuConversionLease<'device> {
    bind_group: GpuBindGroup,
    destination_planes: Vec<GpuTextureView>,
    source_upload: Option<UploadedFrame<'device>>,
    destination_upload: Option<UploadedFrame<'device>>,
}

impl GpuConversionLease<'_> {
    /// Returns the managed bind group retained by this encoded conversion.
    #[must_use]
    pub const fn bind_group(&self) -> &GpuBindGroup {
        &self.bind_group
    }

    /// Returns destination views retained through command submission.
    #[must_use]
    pub fn destination_planes(&self) -> &[GpuTextureView] {
        &self.destination_planes
    }

    /// Returns uploaded source and destination owners retained by this lease.
    #[must_use]
    pub const fn retained_uploads(
        &self,
    ) -> (Option<&UploadedFrame<'_>>, Option<&UploadedFrame<'_>>) {
        (
            self.source_upload.as_ref(),
            self.destination_upload.as_ref(),
        )
    }
}

/// A device-scoped, immutable pixel conversion compiled for one exact plan.
#[derive(Debug)]
pub struct GpuPixelConverter<'device> {
    resources: GpuResources<'device>,
    plan: GpuConversionPlan,
    bind_group_layout: GpuBindGroupLayout,
    pipelines: Vec<GpuRenderPipeline>,
}

impl<'device> GpuPixelConverter<'device> {
    /// Compiles managed exact-format render pipelines for every destination plane.
    pub async fn new(resources: GpuResources<'device>, plan: GpuConversionPlan) -> Result<Self> {
        let capacity =
            NonZeroUsize::new(plan.destination.plane_layouts.len()).ok_or_else(|| {
                invalid(
                    "create_pixel_converter",
                    "pixel conversion requires at least one destination plane",
                )
            })?;
        let shader_cache = ShaderCache::new(&resources, capacity);
        Self::with_shader_cache(resources, &shader_cache, plan).await
    }

    /// Compiles through a caller-owned bounded shader cache for cross-plan reuse.
    pub async fn with_shader_cache(
        resources: GpuResources<'device>,
        shader_cache: &ShaderCache<'device>,
        plan: GpuConversionPlan,
    ) -> Result<Self> {
        validate_plan_device_support(&resources, &plan)?;
        let layout_entries = plan
            .source
            .plane_layouts
            .iter()
            .enumerate()
            .map(|(binding, plane)| wgpu::BindGroupLayoutEntry {
                binding: u32::try_from(binding)
                    .expect("pixel plane counts always fit in a binding index"),
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: plane.sample_type(),
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            })
            .collect::<Vec<_>>();
        let bind_group_layout =
            resources.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("superi pixel conversion inputs"),
                entries: &layout_entries,
            })?;
        let pipeline_layout = resources.create_pipeline_layout(GpuPipelineLayoutDescriptor {
            label: Some("superi pixel conversion layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        })?;

        let mut pipelines = Vec::with_capacity(plan.destination.plane_layouts.len());
        for (plane_index, plane) in plan.destination.plane_layouts.iter().enumerate() {
            let source = shader_source(&plan, plane_index)?;
            let module = shader_cache
                .compile_wgsl(GpuShaderModuleDescriptor {
                    label: Some("superi pixel conversion shader"),
                    source: &source,
                })
                .await?;
            let targets = [Some(wgpu::ColorTargetState {
                format: plane.texture_format,
                blend: None,
                write_mask: wgpu::ColorWrites::ALL,
            })];
            let pipeline = resources
                .create_render_pipeline(GpuRenderPipelineDescriptor {
                    label: Some("superi pixel conversion pipeline"),
                    layout: Some(&pipeline_layout),
                    vertex: GpuVertexState {
                        module: &module,
                        entry_point: "vertex_main",
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        buffers: &[],
                    },
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    fragment: Some(GpuFragmentState {
                        module: &module,
                        entry_point: "fragment_main",
                        compilation_options: wgpu::PipelineCompilationOptions::default(),
                        targets: &targets,
                    }),
                    multiview: None,
                    cache: None,
                })
                .await?;
            pipelines.push(pipeline);
        }

        Ok(Self {
            resources,
            plan,
            bind_group_layout,
            pipelines,
        })
    }

    /// Returns the immutable source and destination contract.
    #[must_use]
    pub const fn plan(&self) -> &GpuConversionPlan {
        &self.plan
    }

    /// Encodes ordered render passes without submitting or reading pixels back.
    pub fn encode(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        source: &GpuPixelFrame<'device>,
        destination: &GpuPixelFrame<'device>,
    ) -> Result<GpuConversionLease<'device>> {
        if source.descriptor != self.plan.source || destination.descriptor != self.plan.destination
        {
            return Err(invalid(
                "encode_conversion",
                "source and destination frames must exactly match the compiled conversion plan",
            ));
        }
        for view in &source.planes {
            if !view
                .texture()
                .info()
                .usage()
                .contains(wgpu::TextureUsages::TEXTURE_BINDING)
            {
                return Err(invalid(
                    "encode_conversion",
                    "every source plane must permit texture binding",
                ));
            }
        }
        for view in &destination.planes {
            self.resources
                .ensure_owner(view.lease(), "encode_conversion")?;
            if !view
                .texture()
                .info()
                .usage()
                .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
            {
                return Err(invalid(
                    "encode_conversion",
                    "every destination plane must permit render attachment use",
                ));
            }
        }

        let entries = source
            .planes
            .iter()
            .enumerate()
            .map(|(binding, view)| {
                GpuBindGroupEntry::texture_view(
                    u32::try_from(binding)
                        .expect("pixel plane counts always fit in a binding index"),
                    view.clone(),
                )
            })
            .collect::<Vec<_>>();
        let bind_group = self.resources.create_bind_group(GpuBindGroupDescriptor {
            label: Some("superi pixel conversion inputs"),
            layout: &self.bind_group_layout,
            entries: &entries,
        })?;

        for (((pipeline, view), layout), plane_index) in self
            .pipelines
            .iter()
            .zip(destination.planes.iter())
            .zip(destination.descriptor.plane_layouts.iter())
            .zip(0_usize..)
        {
            let label = format!("superi pixel conversion plane {plane_index}");
            let attachments = [Some(wgpu::RenderPassColorAttachment {
                view: view.raw(),
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                    store: wgpu::StoreOp::Store,
                },
            })];
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some(&label),
                color_attachments: &attachments,
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });
            pass.set_pipeline(pipeline.raw());
            pass.set_bind_group(0, bind_group.raw(), &[]);
            pass.set_viewport(
                0.0,
                0.0,
                layout.texture_width as f32,
                layout.texture_height as f32,
                0.0,
                1.0,
            );
            pass.set_scissor_rect(0, 0, layout.texture_width, layout.texture_height);
            pass.draw(0..3, 0..1);
        }

        Ok(GpuConversionLease {
            bind_group,
            destination_planes: destination.planes.clone(),
            source_upload: source.retained_upload.clone(),
            destination_upload: destination.retained_upload.clone(),
        })
    }
}

impl GpuConversionPlan {
    /// Validates a storage-only conversion without inventing color behavior.
    pub fn new(source: GpuFrameDescriptor, destination: GpuFrameDescriptor) -> Result<Self> {
        if source.width != destination.width || source.height != destination.height {
            return Err(invalid(
                "create_conversion_plan",
                "pixel conversion cannot change the logical image extent",
            ));
        }
        validate_color_encoding(&source)?;
        validate_color_encoding(&destination)?;
        if source.color_space.primaries() != destination.color_space.primaries()
            || source.color_space.transfer() != destination.color_space.transfer()
        {
            return Err(unsupported(
                "create_conversion_plan",
                "pixel storage conversion cannot change color primaries or transfer function",
            ));
        }
        if source.alpha_mode != AlphaMode::Opaque
            && (destination.alpha_mode == AlphaMode::Opaque
                || !destination.pixel_format.has_alpha())
        {
            return Err(invalid(
                "create_conversion_plan",
                "non-opaque alpha must be composited before conversion to a format without alpha",
            ));
        }
        Ok(Self {
            source,
            destination,
        })
    }

    /// Returns the exact source representation.
    #[must_use]
    pub const fn source(&self) -> &GpuFrameDescriptor {
        &self.source
    }

    /// Returns the exact destination representation.
    #[must_use]
    pub const fn destination(&self) -> &GpuFrameDescriptor {
        &self.destination
    }
}

fn validate_frame_plane(index: usize, view: &GpuTextureView, layout: GpuPlaneLayout) -> Result<()> {
    let texture = view.texture().info();
    let info = view.info();
    let format = info.format().unwrap_or(texture.format());
    if format != layout.texture_format {
        return Err(invalid(
            "create_pixel_frame",
            format!("plane {index} texture format does not match its portable layout"),
        ));
    }
    if texture.dimension() != wgpu::TextureDimension::D2 || texture.sample_count() != 1 {
        return Err(invalid(
            "create_pixel_frame",
            format!("plane {index} must be a single-sample two-dimensional texture"),
        ));
    }
    if info
        .dimension()
        .is_some_and(|dimension| dimension != wgpu::TextureViewDimension::D2)
        || info.aspect() != wgpu::TextureAspect::All
    {
        return Err(invalid(
            "create_pixel_frame",
            format!("plane {index} must use a complete two-dimensional color view"),
        ));
    }

    let remaining_mips = texture
        .mip_level_count()
        .saturating_sub(info.base_mip_level());
    if info.mip_level_count().unwrap_or(remaining_mips) != 1 {
        return Err(invalid(
            "create_pixel_frame",
            format!("plane {index} view must expose exactly one mip level"),
        ));
    }
    let remaining_layers = texture
        .size()
        .depth_or_array_layers
        .saturating_sub(info.base_array_layer());
    if info.array_layer_count().unwrap_or(remaining_layers) != 1 {
        return Err(invalid(
            "create_pixel_frame",
            format!("plane {index} view must expose exactly one array layer"),
        ));
    }
    let width = texture
        .size()
        .width
        .checked_shr(info.base_mip_level())
        .unwrap_or(0)
        .max(1);
    let height = texture
        .size()
        .height
        .checked_shr(info.base_mip_level())
        .unwrap_or(0)
        .max(1);
    if width < layout.texture_width || height < layout.texture_height {
        return Err(invalid(
            "create_pixel_frame",
            format!("plane {index} allocation is smaller than its logical extent"),
        ));
    }
    Ok(())
}

fn validate_plan_device_support(
    resources: &GpuResources<'_>,
    plan: &GpuConversionPlan,
) -> Result<()> {
    for plane in &plan.source.plane_layouts {
        let features = resources.texture_format_features(plane.texture_format);
        if !features
            .allowed_usages
            .contains(wgpu::TextureUsages::TEXTURE_BINDING)
        {
            return Err(unsupported(
                "create_pixel_converter",
                format!(
                    "source plane format {:?} is not sampleable on this device",
                    plane.texture_format
                ),
            ));
        }
    }
    for plane in &plan.destination.plane_layouts {
        let features = resources.texture_format_features(plane.texture_format);
        if !features
            .allowed_usages
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
        {
            return Err(unsupported(
                "create_pixel_converter",
                format!(
                    "destination plane format {:?} is not renderable on this device",
                    plane.texture_format
                ),
            ));
        }
    }
    Ok(())
}

fn shader_source(plan: &GpuConversionPlan, destination_plane: usize) -> Result<String> {
    let mut shader = String::new();
    for (binding, plane) in plan.source.plane_layouts.iter().enumerate() {
        let sampled = if plane_is_uint(*plane) { "u32" } else { "f32" };
        let _ = writeln!(
            shader,
            "@group(0) @binding({binding}) var source_plane_{binding}: texture_2d<{sampled}>;"
        );
    }
    shader.push_str(
        r#"
@vertex
fn vertex_main(@builtin(vertex_index) vertex_index: u32) -> @builtin(position) vec4<f32> {
    var positions = array<vec2<f32>, 3>(
        vec2<f32>(-1.0, -1.0),
        vec2<f32>(3.0, -1.0),
        vec2<f32>(-1.0, 3.0),
    );
    return vec4<f32>(positions[vertex_index], 0.0, 1.0);
}

"#,
    );
    write_source_loader(&mut shader, &plan.source)?;
    write_alpha_conversion(
        &mut shader,
        plan.source.alpha_mode,
        plan.destination.alpha_mode,
    );
    let return_type = write_destination_value(&mut shader, &plan.destination, destination_plane)?;
    let _ = writeln!(
        shader,
        r#"
@fragment
fn fragment_main(@builtin(position) position: vec4<f32>) -> @location(0) vec4<{return_type}> {{
    return destination_value(vec2<u32>(position.xy));
}}
"#
    );
    Ok(shader)
}

fn write_source_loader(shader: &mut String, descriptor: &GpuFrameDescriptor) -> Result<()> {
    if descriptor.pixel_format.model() == PixelModel::Yuv {
        write_yuv_source_loader(shader, descriptor)
    } else {
        write_rgb_source_loader(shader, descriptor)
    }
}

fn write_rgb_source_loader(shader: &mut String, descriptor: &GpuFrameDescriptor) -> Result<()> {
    let integer = descriptor.pixel_format.numeric() == PixelNumeric::Unorm
        && descriptor.pixel_format.bits_per_component() == 16;
    if integer {
        shader.push_str(
            "fn load_source_raw(coord: vec2<i32>) -> vec4<f32> {\n    return vec4<f32>(textureLoad(source_plane_0, coord, 0)) / 65535.0;\n}\n\n",
        );
    } else {
        shader.push_str(
            "fn load_source_raw(coord: vec2<i32>) -> vec4<f32> {\n    return textureLoad(source_plane_0, coord, 0);\n}\n\n",
        );
    }
    let range_expression = match descriptor.color_space.range() {
        ColorRange::Full => "value".to_owned(),
        ColorRange::Limited => {
            let (offset, scale) = rgb_limited_code_range(descriptor.pixel_format);
            format!("(value - vec3<f32>({offset:.10})) / {scale:.10}")
        }
        ColorRange::Unspecified => {
            return Err(unsupported(
                "compile_conversion_shader",
                "RGB source range is unresolved",
            ));
        }
        _ => {
            return Err(unsupported(
                "compile_conversion_shader",
                "RGB source range is not supported by this build",
            ));
        }
    };
    let _ = writeln!(
        shader,
        "fn decode_source_rgb_range(value: vec3<f32>) -> vec3<f32> {{\n    return {range_expression};\n}}\n"
    );

    if matches!(
        descriptor.pixel_format,
        PixelFormat::Rgb8Unorm | PixelFormat::Bgr8Unorm
    ) {
        let rgb = if descriptor.pixel_format == PixelFormat::Rgb8Unorm {
            "vec3<f32>(first, second, third)"
        } else {
            "vec3<f32>(third, second, first)"
        };
        let _ = writeln!(
            shader,
            r#"fn load_source_pixel(coord: vec2<i32>) -> vec4<f32> {{
    let base = vec2<i32>(coord.x * 3, coord.y);
    let first = textureLoad(source_plane_0, base, 0).x;
    let second = textureLoad(source_plane_0, base + vec2<i32>(1, 0), 0).x;
    let third = textureLoad(source_plane_0, base + vec2<i32>(2, 0), 0).x;
    return vec4<f32>(decode_source_rgb_range({rgb}), 1.0);
}}
"#
        );
        return Ok(());
    }

    let pixel = match descriptor.pixel_format {
        PixelFormat::R8Unorm
        | PixelFormat::R16Unorm
        | PixelFormat::R16Float
        | PixelFormat::R32Float => "vec4<f32>(decode_source_rgb_range(vec3<f32>(raw.x)), 1.0)",
        PixelFormat::Rg8Unorm
        | PixelFormat::Rg16Unorm
        | PixelFormat::Rg16Float
        | PixelFormat::Rg32Float => {
            "vec4<f32>(decode_source_rgb_range(vec3<f32>(raw.x, raw.y, 0.0)), 1.0)"
        }
        PixelFormat::Rgb8Unorm => "vec4<f32>(decode_source_rgb_range(raw.xyz), 1.0)",
        PixelFormat::Bgr8Unorm => "vec4<f32>(decode_source_rgb_range(raw.zyx), 1.0)",
        PixelFormat::Rgba8Unorm
        | PixelFormat::Rgba16Unorm
        | PixelFormat::Rgba16Float
        | PixelFormat::Rgba32Float => "vec4<f32>(decode_source_rgb_range(raw.xyz), raw.w)",
        PixelFormat::Bgra8Unorm => "vec4<f32>(decode_source_rgb_range(raw.xyz), raw.w)",
        _ => {
            return Err(unsupported(
                "compile_conversion_shader",
                "RGB source format is not supported by this build",
            ));
        }
    };
    let alpha = if descriptor.alpha_mode == AlphaMode::Opaque {
        "return vec4<f32>(pixel.xyz, 1.0);"
    } else {
        "return pixel;"
    };
    let _ = writeln!(
        shader,
        "fn load_source_pixel(coord: vec2<i32>) -> vec4<f32> {{\n    let raw = load_source_raw(coord);\n    let pixel = {pixel};\n    {alpha}\n}}\n"
    );
    Ok(())
}

fn write_yuv_source_loader(shader: &mut String, descriptor: &GpuFrameDescriptor) -> Result<()> {
    let bits = descriptor.pixel_format.bits_per_component();
    let shift = descriptor.plane_layouts[0].stored_bit_shift;
    let chroma_layout = descriptor.plane_layouts[1];
    let luma_code = if bits == 8 {
        "textureLoad(source_plane_0, coord, 0).x * 255.0".to_owned()
    } else {
        format!("f32(textureLoad(source_plane_0, coord, 0).x >> {shift}u)")
    };
    let _ = writeln!(
        shader,
        "fn load_source_luma_code(coord: vec2<i32>) -> f32 {{\n    return {luma_code};\n}}\n"
    );

    let chroma_code = if descriptor.pixel_format.packing() == PixelPacking::Semiplanar {
        if bits == 8 {
            "textureLoad(source_plane_1, safe, 0).xy * 255.0".to_owned()
        } else {
            format!("vec2<f32>(textureLoad(source_plane_1, safe, 0).xy >> vec2<u32>({shift}u))")
        }
    } else if bits == 8 {
        "vec2<f32>(textureLoad(source_plane_1, safe, 0).x, textureLoad(source_plane_2, safe, 0).x) * 255.0".to_owned()
    } else {
        format!(
            "vec2<f32>(f32(textureLoad(source_plane_1, safe, 0).x >> {shift}u), f32(textureLoad(source_plane_2, safe, 0).x >> {shift}u))"
        )
    };
    let _ = writeln!(
        shader,
        "fn load_source_chroma_at(coord: vec2<i32>) -> vec2<f32> {{\n    let dimensions = vec2<i32>({chroma_width}, {chroma_height});\n    let safe = clamp(coord, vec2<i32>(0, 0), dimensions - vec2<i32>(1, 1));\n    return {chroma_code};\n}}\n",
        chroma_width = chroma_layout.texture_width,
        chroma_height = chroma_layout.texture_height,
    );

    match descriptor.pixel_format.chroma_subsampling() {
        Some(ChromaSubsampling::Cs444) => shader.push_str(
            "fn load_source_chroma(luma_coord: vec2<i32>) -> vec2<f32> {\n    return load_source_chroma_at(luma_coord);\n}\n\n",
        ),
        Some(ChromaSubsampling::Cs420 | ChromaSubsampling::Cs422) => {
            let (block_x, block_y) = chroma_block(descriptor.pixel_format)?;
            let (origin_x, origin_y) = chroma_origin(
                descriptor
                    .chroma_location
                    .expect("subsampled descriptor validation requires location"),
                block_x,
                block_y,
            );
            let _ = writeln!(
                shader,
                r#"fn load_source_chroma(luma_coord: vec2<i32>) -> vec2<f32> {{
    let block = vec2<f32>({block_x:.1}, {block_y:.1});
    let origin = vec2<f32>({origin_x:.1}, {origin_y:.1});
    let position = (vec2<f32>(luma_coord) + vec2<f32>(0.5, 0.5) - origin) / block;
    let base = vec2<i32>(floor(position));
    let fraction = fract(position);
    let upper = mix(
        load_source_chroma_at(base),
        load_source_chroma_at(base + vec2<i32>(1, 0)),
        fraction.x,
    );
    let lower = mix(
        load_source_chroma_at(base + vec2<i32>(0, 1)),
        load_source_chroma_at(base + vec2<i32>(1, 1)),
        fraction.x,
    );
    return mix(upper, lower, fraction.y);
}}
"#
            );
        }
        _ => {
            return Err(unsupported(
                "compile_conversion_shader",
                "YUV source subsampling is not supported by this build",
            ));
        }
    }

    let (y_offset, y_scale, chroma_mid, chroma_scale) =
        yuv_code_range(bits, descriptor.color_space.range())?;
    let _ = writeln!(
        shader,
        r#"fn decode_source_yuv(code: vec3<f32>) -> vec3<f32> {{
    return vec3<f32>(
        (code.x - {y_offset:.10}) / {y_scale:.10},
        (code.y - {chroma_mid:.10}) / {chroma_scale:.10},
        (code.z - {chroma_mid:.10}) / {chroma_scale:.10},
    );
}}
"#
    );
    write_yuv_to_rgb(shader, "source", descriptor.color_space.matrix())?;
    shader.push_str(
        "fn load_source_pixel(coord: vec2<i32>) -> vec4<f32> {\n    let chroma = load_source_chroma(coord);\n    let yuv = decode_source_yuv(vec3<f32>(load_source_luma_code(coord), chroma));\n    return vec4<f32>(source_yuv_to_rgb(yuv), 1.0);\n}\n\n",
    );
    Ok(())
}

fn write_alpha_conversion(shader: &mut String, source: AlphaMode, destination: AlphaMode) {
    let body = match (source, destination) {
        (AlphaMode::Straight, AlphaMode::Premultiplied) => {
            "return vec4<f32>(pixel.xyz * pixel.w, pixel.w);"
        }
        (AlphaMode::Premultiplied, AlphaMode::Straight) => {
            "if (pixel.w <= 0.0) { return vec4<f32>(0.0); }\n    return vec4<f32>(pixel.xyz / pixel.w, pixel.w);"
        }
        (_, AlphaMode::Opaque) => "return vec4<f32>(pixel.xyz, 1.0);",
        _ => "return pixel;",
    };
    let _ = writeln!(
        shader,
        "fn convert_alpha(pixel: vec4<f32>) -> vec4<f32> {{\n    {body}\n}}\n"
    );
}

fn write_destination_value(
    shader: &mut String,
    descriptor: &GpuFrameDescriptor,
    plane_index: usize,
) -> Result<&'static str> {
    if descriptor.pixel_format.model() == PixelModel::Yuv {
        write_yuv_destination_value(shader, descriptor, plane_index)
    } else {
        write_rgb_destination_value(shader, descriptor)
    }
}

fn write_rgb_destination_value(
    shader: &mut String,
    descriptor: &GpuFrameDescriptor,
) -> Result<&'static str> {
    let range_expression = match descriptor.color_space.range() {
        ColorRange::Full => "value".to_owned(),
        ColorRange::Limited => {
            let (offset, scale) = rgb_limited_code_range(descriptor.pixel_format);
            format!("value * {scale:.10} + vec3<f32>({offset:.10})")
        }
        ColorRange::Unspecified => {
            return Err(unsupported(
                "compile_conversion_shader",
                "RGB destination range is unresolved",
            ));
        }
        _ => {
            return Err(unsupported(
                "compile_conversion_shader",
                "RGB destination range is not supported by this build",
            ));
        }
    };
    let _ = writeln!(
        shader,
        "fn encode_destination_rgb_range(value: vec3<f32>) -> vec3<f32> {{\n    return {range_expression};\n}}\n"
    );

    if matches!(
        descriptor.pixel_format,
        PixelFormat::Rgb8Unorm | PixelFormat::Bgr8Unorm
    ) {
        let components = if descriptor.pixel_format == PixelFormat::Rgb8Unorm {
            "rgb"
        } else {
            "rgb.zyx"
        };
        let _ = writeln!(
            shader,
            r#"fn destination_value(coord: vec2<u32>) -> vec4<f32> {{
    let logical_coord = vec2<u32>(coord.x / 3u, coord.y);
    let pixel = convert_alpha(load_source_pixel(vec2<i32>(logical_coord)));
    let rgb = encode_destination_rgb_range(pixel.xyz);
    let components = {components};
    var value = components.x;
    if (coord.x % 3u == 1u) {{ value = components.y; }}
    if (coord.x % 3u == 2u) {{ value = components.z; }}
    return vec4<f32>(value, 0.0, 0.0, 1.0);
}}
"#
        );
        return Ok("f32");
    }

    let normalized = match descriptor.pixel_format {
        PixelFormat::R8Unorm
        | PixelFormat::R16Unorm
        | PixelFormat::R16Float
        | PixelFormat::R32Float => "vec4<f32>(rgb.x, 0.0, 0.0, 1.0)",
        PixelFormat::Rg8Unorm
        | PixelFormat::Rg16Unorm
        | PixelFormat::Rg16Float
        | PixelFormat::Rg32Float => "vec4<f32>(rgb.xy, 0.0, 1.0)",
        PixelFormat::Rgb8Unorm
        | PixelFormat::Rgba8Unorm
        | PixelFormat::Rgba16Unorm
        | PixelFormat::Rgba16Float
        | PixelFormat::Rgba32Float => "vec4<f32>(rgb, pixel.w)",
        PixelFormat::Bgra8Unorm => "vec4<f32>(rgb, pixel.w)",
        _ => {
            return Err(unsupported(
                "compile_conversion_shader",
                "RGB destination format is not supported by this build",
            ));
        }
    };
    let uint = descriptor.pixel_format.numeric() == PixelNumeric::Unorm
        && descriptor.pixel_format.bits_per_component() == 16;
    if uint {
        let _ = writeln!(
            shader,
            r#"fn destination_value(coord: vec2<u32>) -> vec4<u32> {{
    let pixel = convert_alpha(load_source_pixel(vec2<i32>(coord)));
    let rgb = encode_destination_rgb_range(pixel.xyz);
    let normalized = {normalized};
    return vec4<u32>(round(clamp(normalized, vec4<f32>(0.0), vec4<f32>(1.0)) * 65535.0));
}}
"#
        );
        Ok("u32")
    } else {
        let _ = writeln!(
            shader,
            r#"fn destination_value(coord: vec2<u32>) -> vec4<f32> {{
    let pixel = convert_alpha(load_source_pixel(vec2<i32>(coord)));
    let rgb = encode_destination_rgb_range(pixel.xyz);
    return {normalized};
}}
"#
        );
        Ok("f32")
    }
}

fn write_yuv_destination_value(
    shader: &mut String,
    descriptor: &GpuFrameDescriptor,
    plane_index: usize,
) -> Result<&'static str> {
    write_rgb_to_yuv(shader, "destination", descriptor.color_space.matrix())?;
    let bits = descriptor.pixel_format.bits_per_component();
    let (y_offset, y_scale, chroma_mid, chroma_scale) =
        yuv_code_range(bits, descriptor.color_space.range())?;
    let _ = writeln!(
        shader,
        r#"fn encode_destination_yuv(value: vec3<f32>) -> vec3<f32> {{
    return vec3<f32>(
        value.x * {y_scale:.10} + {y_offset:.10},
        value.y * {chroma_scale:.10} + {chroma_mid:.10},
        value.z * {chroma_scale:.10} + {chroma_mid:.10},
    );
}}
"#
    );

    if plane_index > 0 {
        let (block_x, block_y) = chroma_block(descriptor.pixel_format)?;
        let location = descriptor.chroma_location;
        let (sample_width, sample_height) = match location {
            Some(ChromaLocation::TopLeft) | None => (1, 1),
            Some(ChromaLocation::Left) => (1, block_y),
            Some(ChromaLocation::Center) => (block_x, block_y),
        };
        let _ = writeln!(
            shader,
            r#"fn destination_chroma_rgb(chroma_coord: vec2<u32>) -> vec3<f32> {{
    let base = chroma_coord * vec2<u32>({block_x}u, {block_y}u);
    var sum = vec3<f32>(0.0);
    var count = 0.0;
    for (var y = 0u; y < {sample_height}u; y = y + 1u) {{
        for (var x = 0u; x < {sample_width}u; x = x + 1u) {{
            let coord = base + vec2<u32>(x, y);
            if (coord.x < {width}u && coord.y < {height}u) {{
                sum = sum + convert_alpha(load_source_pixel(vec2<i32>(coord))).xyz;
                count = count + 1.0;
            }}
        }}
    }}
    return sum / count;
}}
"#,
            width = descriptor.width,
            height = descriptor.height,
        );
    }

    let semiplanar = descriptor.pixel_format.packing() == PixelPacking::Semiplanar;
    let code_value = if plane_index == 0 {
        "let rgb = convert_alpha(load_source_pixel(vec2<i32>(coord))).xyz;\n    let code = encode_destination_yuv(destination_rgb_to_yuv(rgb));\n    let value = vec4<f32>(code.x, 0.0, 0.0, 1.0);"
            .to_owned()
    } else if semiplanar {
        "let rgb = destination_chroma_rgb(coord);\n    let code = encode_destination_yuv(destination_rgb_to_yuv(rgb));\n    let value = vec4<f32>(code.y, code.z, 0.0, 1.0);"
            .to_owned()
    } else if plane_index == 1 {
        "let rgb = destination_chroma_rgb(coord);\n    let code = encode_destination_yuv(destination_rgb_to_yuv(rgb));\n    let value = vec4<f32>(code.y, 0.0, 0.0, 1.0);"
            .to_owned()
    } else {
        "let rgb = destination_chroma_rgb(coord);\n    let code = encode_destination_yuv(destination_rgb_to_yuv(rgb));\n    let value = vec4<f32>(code.z, 0.0, 0.0, 1.0);"
            .to_owned()
    };
    if bits == 8 {
        let _ = writeln!(
            shader,
            "fn destination_value(coord: vec2<u32>) -> vec4<f32> {{\n    {code_value}\n    return value / 255.0;\n}}\n"
        );
        Ok("f32")
    } else {
        let shift = descriptor.plane_layouts[plane_index].stored_bit_shift;
        let _ = writeln!(
            shader,
            "fn destination_value(coord: vec2<u32>) -> vec4<u32> {{\n    {code_value}\n    let quantized = vec4<u32>(round(clamp(value, vec4<f32>(0.0), vec4<f32>(1023.0))));\n    return quantized << vec4<u32>({shift}u);\n}}\n"
        );
        Ok("u32")
    }
}

fn write_yuv_to_rgb(shader: &mut String, prefix: &str, matrix: MatrixCoefficients) -> Result<()> {
    if matrix == MatrixCoefficients::Bt2020Constant {
        let _ = writeln!(
            shader,
            r#"fn {prefix}_yuv_to_rgb(yuv: vec3<f32>) -> vec3<f32> {{
    var blue = yuv.x + yuv.y * 1.5816;
    if (yuv.y <= 0.0) {{ blue = yuv.x + yuv.y * 1.9404; }}
    var red = yuv.x + yuv.z * 0.9936;
    if (yuv.z <= 0.0) {{ red = yuv.x + yuv.z * 1.7184; }}
    let green = (yuv.x - 0.2627 * red - 0.0593 * blue) / 0.6780;
    return vec3<f32>(red, green, blue);
}}
"#
        );
        return Ok(());
    }
    let (kr, kb) = matrix_coefficients(matrix)?;
    let kg = 1.0 - kr - kb;
    let red = 2.0 * (1.0 - kr);
    let blue = 2.0 * (1.0 - kb);
    let green_cb = 2.0 * kb * (1.0 - kb) / kg;
    let green_cr = 2.0 * kr * (1.0 - kr) / kg;
    let _ = writeln!(
        shader,
        r#"fn {prefix}_yuv_to_rgb(yuv: vec3<f32>) -> vec3<f32> {{
    return vec3<f32>(
        yuv.x + {red:.10} * yuv.z,
        yuv.x - {green_cb:.10} * yuv.y - {green_cr:.10} * yuv.z,
        yuv.x + {blue:.10} * yuv.y,
    );
}}
"#
    );
    Ok(())
}

fn write_rgb_to_yuv(shader: &mut String, prefix: &str, matrix: MatrixCoefficients) -> Result<()> {
    if matrix == MatrixCoefficients::Bt2020Constant {
        let _ = writeln!(
            shader,
            r#"fn {prefix}_rgb_to_yuv(rgb: vec3<f32>) -> vec3<f32> {{
    let luma = dot(rgb, vec3<f32>(0.2627, 0.6780, 0.0593));
    var cb = (rgb.z - luma) / 1.5816;
    if (rgb.z <= luma) {{ cb = (rgb.z - luma) / 1.9404; }}
    var cr = (rgb.x - luma) / 0.9936;
    if (rgb.x <= luma) {{ cr = (rgb.x - luma) / 1.7184; }}
    return vec3<f32>(luma, cb, cr);
}}
"#
        );
        return Ok(());
    }
    let (kr, kb) = matrix_coefficients(matrix)?;
    let kg = 1.0 - kr - kb;
    let cb_scale = 2.0 * (1.0 - kb);
    let cr_scale = 2.0 * (1.0 - kr);
    let _ = writeln!(
        shader,
        r#"fn {prefix}_rgb_to_yuv(rgb: vec3<f32>) -> vec3<f32> {{
    let luma = dot(rgb, vec3<f32>({kr:.10}, {kg:.10}, {kb:.10}));
    return vec3<f32>(luma, (rgb.z - luma) / {cb_scale:.10}, (rgb.x - luma) / {cr_scale:.10});
}}
"#
    );
    Ok(())
}

fn matrix_coefficients(matrix: MatrixCoefficients) -> Result<(f32, f32)> {
    match matrix {
        MatrixCoefficients::Bt601 => Ok((0.299, 0.114)),
        MatrixCoefficients::Bt709 => Ok((0.2126, 0.0722)),
        MatrixCoefficients::Bt2020NonConstant => Ok((0.2627, 0.0593)),
        _ => Err(unsupported(
            "compile_conversion_shader",
            "matrix coefficients are not supported by this build",
        )),
    }
}

fn yuv_code_range(bits: u8, range: ColorRange) -> Result<(f32, f32, f32, f32)> {
    let scale = 2_f32.powi(i32::from(bits) - 8);
    match range {
        ColorRange::Full => {
            let maximum = 2_f32.powi(i32::from(bits)) - 1.0;
            Ok((0.0, maximum, 2_f32.powi(i32::from(bits) - 1), maximum))
        }
        ColorRange::Limited => Ok((16.0 * scale, 219.0 * scale, 128.0 * scale, 224.0 * scale)),
        ColorRange::Unspecified => Err(unsupported(
            "compile_conversion_shader",
            "YUV code range is unresolved",
        )),
        _ => Err(unsupported(
            "compile_conversion_shader",
            "YUV code range is not supported by this build",
        )),
    }
}

fn rgb_limited_code_range(format: PixelFormat) -> (f32, f32) {
    let bits = if format.numeric() == PixelNumeric::Unorm {
        format.bits_per_component()
    } else {
        8
    };
    let code_scale = 2_f32.powi(i32::from(bits) - 8);
    let maximum = 2_f32.powi(i32::from(bits)) - 1.0;
    (16.0 * code_scale / maximum, 219.0 * code_scale / maximum)
}

fn chroma_block(format: PixelFormat) -> Result<(u32, u32)> {
    match format.chroma_subsampling() {
        Some(ChromaSubsampling::Cs420) => Ok((2, 2)),
        Some(ChromaSubsampling::Cs422) => Ok((2, 1)),
        Some(ChromaSubsampling::Cs444) => Ok((1, 1)),
        _ => Err(unsupported(
            "compile_conversion_shader",
            "chroma subsampling is not supported by this build",
        )),
    }
}

fn chroma_origin(location: ChromaLocation, block_x: u32, block_y: u32) -> (f32, f32) {
    match location {
        ChromaLocation::Left => (0.5, block_y as f32 / 2.0),
        ChromaLocation::Center => (block_x as f32 / 2.0, block_y as f32 / 2.0),
        ChromaLocation::TopLeft => (0.5, 0.5),
    }
}

fn plane_is_uint(layout: GpuPlaneLayout) -> bool {
    matches!(
        layout.texture_format,
        wgpu::TextureFormat::R16Uint
            | wgpu::TextureFormat::Rg16Uint
            | wgpu::TextureFormat::Rgba16Uint
    )
}

fn validate_color_encoding(descriptor: &GpuFrameDescriptor) -> Result<()> {
    let matrix = descriptor.color_space.matrix();
    let range = descriptor.color_space.range();
    if range == ColorRange::Unspecified {
        return Err(unsupported(
            "create_conversion_plan",
            "pixel conversion requires an explicit full or limited code range",
        ));
    }

    if descriptor.pixel_format.model() == PixelModel::Yuv {
        if !matches!(
            matrix,
            MatrixCoefficients::Bt601
                | MatrixCoefficients::Bt709
                | MatrixCoefficients::Bt2020NonConstant
                | MatrixCoefficients::Bt2020Constant
        ) {
            return Err(unsupported(
                "create_conversion_plan",
                "YUV conversion requires explicit supported matrix coefficients",
            ));
        }
    } else if matrix != MatrixCoefficients::Rgb {
        return Err(unsupported(
            "create_conversion_plan",
            "RGB conversion requires explicit RGB matrix signaling",
        ));
    }
    Ok(())
}

fn plane_layouts(width: u32, height: u32, format: PixelFormat) -> Result<Vec<GpuPlaneLayout>> {
    let single = |texture_format, valid_bits| {
        vec![GpuPlaneLayout {
            width,
            height,
            texture_width: width,
            texture_height: height,
            texture_format,
            valid_bits,
            stored_bit_shift: 0,
        }]
    };

    let packed = match format {
        PixelFormat::R8Unorm => Some(single(wgpu::TextureFormat::R8Unorm, 8)),
        PixelFormat::R16Unorm => Some(single(wgpu::TextureFormat::R16Uint, 16)),
        PixelFormat::R16Float => Some(single(wgpu::TextureFormat::R16Float, 16)),
        PixelFormat::R32Float => Some(single(wgpu::TextureFormat::R32Float, 32)),
        PixelFormat::Rg8Unorm => Some(single(wgpu::TextureFormat::Rg8Unorm, 8)),
        PixelFormat::Rg16Unorm => Some(single(wgpu::TextureFormat::Rg16Uint, 16)),
        PixelFormat::Rg16Float => Some(single(wgpu::TextureFormat::Rg16Float, 16)),
        PixelFormat::Rg32Float => Some(single(wgpu::TextureFormat::Rg32Float, 32)),
        PixelFormat::Rgb8Unorm | PixelFormat::Bgr8Unorm => {
            let texture_width = width.checked_mul(3).ok_or_else(|| {
                Error::new(
                    ErrorCategory::ResourceExhausted,
                    Recoverability::UserCorrectable,
                    "packed RGB texture width overflowed",
                )
                .with_context(ErrorContext::new(COMPONENT, "create_frame_descriptor"))
            })?;
            Some(vec![GpuPlaneLayout {
                width,
                height,
                texture_width,
                texture_height: height,
                texture_format: wgpu::TextureFormat::R8Unorm,
                valid_bits: 8,
                stored_bit_shift: 0,
            }])
        }
        PixelFormat::Rgba8Unorm => Some(single(wgpu::TextureFormat::Rgba8Unorm, 8)),
        PixelFormat::Bgra8Unorm => Some(single(wgpu::TextureFormat::Bgra8Unorm, 8)),
        PixelFormat::Rgba16Unorm => Some(single(wgpu::TextureFormat::Rgba16Uint, 16)),
        PixelFormat::Rgba16Float => Some(single(wgpu::TextureFormat::Rgba16Float, 16)),
        PixelFormat::Rgba32Float => Some(single(wgpu::TextureFormat::Rgba32Float, 32)),
        PixelFormat::Yuv420p8
        | PixelFormat::Yuv420p10
        | PixelFormat::Yuv422p8
        | PixelFormat::Yuv422p10
        | PixelFormat::Yuv444p8
        | PixelFormat::Yuv444p10
        | PixelFormat::Nv12
        | PixelFormat::P010 => None,
        _ => {
            return Err(unsupported(
                "create_frame_descriptor",
                "the pixel format is not supported by this build",
            ));
        }
    };
    if let Some(packed) = packed {
        return Ok(packed);
    }

    let (chroma_width, chroma_height) = match format.chroma_subsampling() {
        Some(ChromaSubsampling::Cs420) => (width.div_ceil(2), height.div_ceil(2)),
        Some(ChromaSubsampling::Cs422) => (width.div_ceil(2), height),
        Some(ChromaSubsampling::Cs444) => (width, height),
        _ => {
            return Err(unsupported(
                "create_frame_descriptor",
                "YUV format does not expose supported chroma subsampling",
            ));
        }
    };
    let ten_bit = format.bits_per_component() == 10;
    let shift = if format == PixelFormat::P010 { 6 } else { 0 };
    let luma_format = if ten_bit {
        wgpu::TextureFormat::R16Uint
    } else {
        wgpu::TextureFormat::R8Unorm
    };
    let chroma_format = if format.packing() == PixelPacking::Semiplanar {
        if ten_bit {
            wgpu::TextureFormat::Rg16Uint
        } else {
            wgpu::TextureFormat::Rg8Unorm
        }
    } else {
        luma_format
    };
    let mut layouts = vec![GpuPlaneLayout {
        width,
        height,
        texture_width: width,
        texture_height: height,
        texture_format: luma_format,
        valid_bits: format.bits_per_component(),
        stored_bit_shift: shift,
    }];
    let chroma_plane = GpuPlaneLayout {
        width: chroma_width,
        height: chroma_height,
        texture_width: chroma_width,
        texture_height: chroma_height,
        texture_format: chroma_format,
        valid_bits: format.bits_per_component(),
        stored_bit_shift: shift,
    };
    layouts.push(chroma_plane);
    if format.packing() == PixelPacking::Planar {
        layouts.push(chroma_plane);
    }
    Ok(layouts)
}

fn invalid(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn unsupported(operation: &'static str, message: impl Into<String>) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
