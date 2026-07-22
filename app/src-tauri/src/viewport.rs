//! Native viewport window ownership and GPU-resident presentation.

use std::sync::{mpsc, Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use superi_color::gpu_display::{GpuDisplayPresenter, GpuDisplayView};
#[cfg(target_os = "macos")]
use superi_color::icc::SystemDisplayProfileDiscovery;
use superi_color::icc::{
    DisplayProfileCatalog, DisplayProfileSnapshot, IccDisplayModel, IccRenderingIntent, MonitorId,
    MonitorPresentationBinding, PresentationProfileState,
};
use superi_color::transform_out::{OutputColorTransform, OutputTargetKind, OutputTransformOptions};
use superi_color::working_space::WorkingSpace;
use superi_concurrency::threads::{ExecutionDomain, ExecutionDomainThread};
use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::AlphaMode;
use superi_gpu::device::{AdapterSelection, DeviceRequest, GpuInstance, InstanceOptions};
use superi_gpu::resource::GpuResources;
use superi_gpu::submission::GpuSubmissionQueue;
use superi_gpu::surface::{NativeViewportSurface, ViewportExtent};
use superi_gpu::wgpu;
use superi_image::metadata::{
    ColorPipelineMetadata, ColorTransformStage, ColorTransformStageKind, ImageColorTags,
};
use tauri::window::{Monitor, WindowBuilder};
use tauri::{App, Manager, PhysicalPosition, PhysicalSize, State, Window, Wry};

use crate::process_runtime::{
    DesktopProcessRuntime, DesktopProcessServiceId, DesktopProcessServicePhase,
};

const COMPONENT: &str = "superi-desktop.viewport";
const SRGB_DISPLAY_TRANSFORM_ID: &str = "superi.viewport.acescg-to-srgb.v1";
const DISPLAY_P3_TRANSFORM_ID: &str = "superi.viewport.acescg-to-display-p3.v1";
const TRANSFORM_ORDER: [&str; 6] = [
    "alpha_unassociate",
    "scene_to_display_primaries",
    "gamut_mapping",
    "tone_mapping",
    "transfer_encoding",
    "alpha_reassociate",
];
const PROFILE_IDENTITY_NOTE: &str = "Profile identity and freshness verified; built-in display transform selected; arbitrary ICC tag evaluation is unavailable.";
const UNPROFILED_NOTE: &str = "The selected monitor is explicitly unprofiled; the built-in display transform remains explicit and arbitrary ICC tag evaluation is unavailable.";
const PROFILE_UNAVAILABLE_NOTE: &str =
    "Monitor profile discovery is unavailable on this desktop target.";

/// Stable application roles for native GPU media presentation.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopViewerRole {
    Source,
    Program,
    Composite,
    Color,
}

impl DesktopViewerRole {
    pub const ALL: &'static [Self] = &[Self::Source, Self::Program, Self::Composite, Self::Color];

    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Source => "source",
            Self::Program => "program",
            Self::Composite => "composite",
            Self::Color => "color",
        }
    }

    const fn window_label(self) -> &'static str {
        match self {
            Self::Source => "native-source-viewport",
            Self::Program => "native-program-viewport",
            Self::Composite => "native-composite-viewport",
            Self::Color => "native-color-viewport",
        }
    }

    const fn external_window_label(self) -> &'static str {
        match self {
            Self::Source => "external-source-viewport",
            Self::Program => "external-program-viewport",
            Self::Composite => "external-composite-viewport",
            Self::Color => "external-color-viewport",
        }
    }

    const fn source_label(self) -> &'static str {
        match self {
            Self::Source => "source viewer canonical render result",
            Self::Program => "program viewer canonical render result",
            Self::Composite => "composite viewer canonical render result",
            Self::Color => "color viewer canonical render result",
        }
    }

    const fn clear_color(self) -> wgpu::Color {
        match self {
            Self::Source => wgpu::Color {
                r: 0.05,
                g: 0.12,
                b: 0.24,
                a: 1.0,
            },
            Self::Program => wgpu::Color {
                r: 0.18,
                g: 0.035,
                b: 0.012,
                a: 1.0,
            },
            Self::Composite => wgpu::Color {
                r: 0.035,
                g: 0.16,
                b: 0.08,
                a: 1.0,
            },
            Self::Color => wgpu::Color {
                r: 0.2,
                g: 0.07,
                b: 0.2,
                a: 1.0,
            },
        }
    }
}

/// One native presentation destination owned by the GPU submission domain.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum DesktopViewportSurfaceDestination {
    Inline,
    External,
}

impl DesktopViewportSurfaceDestination {
    pub const ALL: &'static [Self] = &[Self::Inline, Self::External];

    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::External => "external",
        }
    }
}

/// Stable shell selection for GPU-resident viewer analysis.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DesktopViewerAnalysisView {
    Image,
    Alpha,
    Red,
    Green,
    Blue,
    Luminance,
    FalseColor,
    Clipping,
}

impl DesktopViewerAnalysisView {
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

    #[must_use]
    pub const fn code(self) -> &'static str {
        self.gpu_view().code()
    }

    #[must_use]
    pub const fn gpu_view(self) -> GpuDisplayView {
        match self {
            Self::Image => GpuDisplayView::Image,
            Self::Alpha => GpuDisplayView::Alpha,
            Self::Red => GpuDisplayView::Red,
            Self::Green => GpuDisplayView::Green,
            Self::Blue => GpuDisplayView::Blue,
            Self::Luminance => GpuDisplayView::Luminance,
            Self::FalseColor => GpuDisplayView::FalseColor,
            Self::Clipping => GpuDisplayView::Clipping,
        }
    }
}

/// Stable built-in transforms supported by the native viewer presenter.
#[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ViewerDisplayTransform {
    Srgb,
    DisplayP3,
}

impl ViewerDisplayTransform {
    pub const ALL: &'static [Self] = &[Self::Srgb, Self::DisplayP3];

    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Srgb => "srgb",
            Self::DisplayP3 => "display_p3",
        }
    }

    #[must_use]
    pub const fn destination(self) -> ColorSpace {
        match self {
            Self::Srgb => ColorSpace::SRGB,
            Self::DisplayP3 => ColorSpace::DISPLAY_P3,
        }
    }

    #[must_use]
    pub const fn transform_id(self) -> &'static str {
        match self {
            Self::Srgb => SRGB_DISPLAY_TRANSFORM_ID,
            Self::DisplayP3 => DISPLAY_P3_TRANSFORM_ID,
        }
    }

    #[must_use]
    pub const fn intent(self) -> &'static str {
        match self {
            Self::Srgb => "scene-linear ACEScg to sRGB display",
            Self::DisplayP3 => "scene-linear ACEScg to Display P3 display",
        }
    }
}

/// Immutable scene and display meaning for one GPU-resident viewer result.
#[derive(Clone, Debug, PartialEq)]
pub struct ViewerPresentationIntent {
    role: DesktopViewerRole,
    source_extent: wgpu::Extent3d,
    display_transform_kind: ViewerDisplayTransform,
    scene_pipeline: ColorPipelineMetadata,
    display_pipeline: ColorPipelineMetadata,
    display_transform: OutputColorTransform,
}

impl ViewerPresentationIntent {
    /// Creates the canonical ACEScg-to-sRGB display intent without rewriting scene metadata.
    pub fn canonical(role: DesktopViewerRole, source_extent: wgpu::Extent3d) -> Result<Self> {
        Self::for_display(role, source_extent, ViewerDisplayTransform::Srgb)
    }

    /// Creates one explicit built-in display branch without rewriting scene metadata.
    pub fn for_display(
        role: DesktopViewerRole,
        source_extent: wgpu::Extent3d,
        display_transform_kind: ViewerDisplayTransform,
    ) -> Result<Self> {
        if source_extent.width == 0
            || source_extent.height == 0
            || source_extent.depth_or_array_layers != 1
        {
            return Err(invalid(
                "create_viewer_presentation_intent",
                "viewer source extents must be nonzero single-layer 2D images",
            ));
        }
        let scene_pipeline = ColorPipelineMetadata::new(ImageColorTags::new(ColorSpace::ACESCG))?;
        let display_stage = ColorTransformStage::new(
            ColorTransformStageKind::Display,
            display_transform_kind.transform_id(),
            ColorSpace::ACESCG,
            display_transform_kind.destination(),
        )?;
        let display_pipeline = scene_pipeline.clone().with_stage(display_stage)?;
        let display_transform = OutputColorTransform::new(
            OutputTargetKind::Display,
            WorkingSpace::ACESCG,
            display_transform_kind.destination(),
            OutputTransformOptions::new(),
        )?;
        Ok(Self {
            role,
            source_extent,
            display_transform_kind,
            scene_pipeline,
            display_pipeline,
            display_transform,
        })
    }

    #[must_use]
    pub const fn role(&self) -> DesktopViewerRole {
        self.role
    }

    #[must_use]
    pub const fn source_extent(&self) -> wgpu::Extent3d {
        self.source_extent
    }

    #[must_use]
    pub const fn source_format(&self) -> wgpu::TextureFormat {
        wgpu::TextureFormat::Rgba16Float
    }

    #[must_use]
    pub const fn alpha_mode(&self) -> AlphaMode {
        AlphaMode::Premultiplied
    }

    #[must_use]
    pub const fn scene_space(&self) -> ColorSpace {
        self.scene_pipeline.current_space()
    }

    #[must_use]
    pub const fn display_space(&self) -> ColorSpace {
        self.display_pipeline.current_space()
    }

    #[must_use]
    pub const fn display_target(&self) -> &'static str {
        "display"
    }

    #[must_use]
    pub const fn display_transform_code(&self) -> &'static str {
        self.display_transform_kind.code()
    }

    #[must_use]
    pub const fn display_intent(&self) -> &'static str {
        self.display_transform_kind.intent()
    }

    #[must_use]
    pub const fn transform_order(&self) -> &'static [&'static str] {
        &TRANSFORM_ORDER
    }

    #[must_use]
    pub fn scene_stage_count(&self) -> usize {
        self.scene_pipeline.stages().len()
    }

    #[must_use]
    pub fn display_stage_kind(&self) -> &'static str {
        self.display_pipeline
            .stages()
            .last()
            .map_or("", |stage| stage.kind().code())
    }

    #[must_use]
    pub fn display_transform_id(&self) -> &str {
        self.display_pipeline
            .stages()
            .last()
            .map_or("", ColorTransformStage::transform_id)
    }

    const fn display_transform(&self) -> OutputColorTransform {
        self.display_transform
    }
}

struct ViewerGpuRenderResult {
    intent: ViewerPresentationIntent,
    texture: superi_gpu::texture::GpuTexture,
}

impl ViewerGpuRenderResult {
    fn new(
        intent: ViewerPresentationIntent,
        texture: superi_gpu::texture::GpuTexture,
    ) -> Result<Self> {
        let info = texture.info();
        if info.size() != intent.source_extent()
            || info.format() != intent.source_format()
            || info.dimension() != wgpu::TextureDimension::D2
            || info.mip_level_count() != 1
            || info.sample_count() != 1
            || !info.usage().contains(wgpu::TextureUsages::TEXTURE_BINDING)
        {
            return Err(invalid(
                "bind_viewer_render_result",
                "viewer render results must retain the canonical managed RGBA16F descriptor",
            ));
        }
        Ok(Self { intent, texture })
    }
}

/// Analysis selection, geometry, and visibility published by the React workspace shell.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopViewportPlacement {
    role: DesktopViewerRole,
    view: DesktopViewerAnalysisView,
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    scale_factor: f64,
    visible: bool,
    external_display_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
struct DesktopMonitorDescriptor {
    name: Option<String>,
    position_x: i32,
    position_y: i32,
    physical_width: u32,
    physical_height: u32,
    scale_factor: f64,
    primary: bool,
}

impl DesktopMonitorDescriptor {
    fn new(
        name: Option<String>,
        position_x: i32,
        position_y: i32,
        physical_width: u32,
        physical_height: u32,
        scale_factor: f64,
        primary: bool,
    ) -> Result<Self> {
        if physical_width == 0
            || physical_height == 0
            || !scale_factor.is_finite()
            || scale_factor <= 0.0
        {
            return Err(invalid(
                "describe_external_display",
                "external display geometry requires nonzero physical dimensions and a finite positive scale",
            ));
        }
        Ok(Self {
            name,
            position_x,
            position_y,
            physical_width,
            physical_height,
            scale_factor,
            primary,
        })
    }

    fn from_monitor(monitor: &Monitor, primary_id: Option<&str>) -> Result<Self> {
        let mut descriptor = Self::new(
            monitor.name().cloned(),
            monitor.position().x,
            monitor.position().y,
            monitor.size().width,
            monitor.size().height,
            monitor.scale_factor(),
            false,
        )?;
        descriptor.primary = primary_id.is_some_and(|id| descriptor.routing_id() == id);
        Ok(descriptor)
    }

    fn routing_id(&self) -> String {
        let identity = format!(
            "{}\u{0}{}\u{0}{}\u{0}{}\u{0}{}\u{0}{:016x}",
            self.name.as_deref().unwrap_or(""),
            self.position_x,
            self.position_y,
            self.physical_width,
            self.physical_height,
            self.scale_factor.to_bits(),
        );
        let digest = Sha256::digest(identity.as_bytes());
        let hexadecimal = digest
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        format!("tauri-monitor:{hexadecimal}")
    }

    fn display_name(&self) -> String {
        self.name
            .as_deref()
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .map(str::to_owned)
            .unwrap_or_else(|| format!("Display at {},{}", self.position_x, self.position_y))
    }
}

/// One connection-local target that can host clean native viewer output.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopExternalDisplayTarget {
    id: String,
    name: String,
    position_x: i32,
    position_y: i32,
    physical_width: u32,
    physical_height: u32,
    scale_factor: f64,
    primary: bool,
}

impl From<&DesktopMonitorDescriptor> for DesktopExternalDisplayTarget {
    fn from(descriptor: &DesktopMonitorDescriptor) -> Self {
        Self {
            id: descriptor.routing_id(),
            name: descriptor.display_name(),
            position_x: descriptor.position_x,
            position_y: descriptor.position_y,
            physical_width: descriptor.physical_width,
            physical_height: descriptor.physical_height,
            scale_factor: descriptor.scale_factor,
            primary: descriptor.primary,
        }
    }
}

/// Exact external surface diagnostics returned beside the inline viewer snapshot.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopExternalOutputSnapshot {
    phase: String,
    target_id: Option<String>,
    target_name: Option<String>,
    selected_view: DesktopViewerAnalysisView,
    presented_view: Option<DesktopViewerAnalysisView>,
    physical_width: u32,
    physical_height: u32,
    scale_factor: f64,
    surface_generation: u64,
    frame_sequence: u64,
    display_intent: String,
    summary: Option<String>,
}

impl Default for DesktopExternalOutputSnapshot {
    fn default() -> Self {
        Self {
            phase: "inactive".to_owned(),
            target_id: None,
            target_name: None,
            selected_view: DesktopViewerAnalysisView::Image,
            presented_view: None,
            physical_width: 0,
            physical_height: 0,
            scale_factor: 0.0,
            surface_generation: 0,
            frame_sequence: 0,
            display_intent: "scene-linear ACEScg to sRGB display".to_owned(),
            summary: Some("No external display selected.".to_owned()),
        }
    }
}

/// Strict shell control for one role's monitor profile and display transform.
#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct DesktopViewportColorSelection {
    role: DesktopViewerRole,
    monitor_id: String,
    display_transform: ViewerDisplayTransform,
}

/// Bounded monitor profile evidence returned to the shell without ICC bytes.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopMonitorProfile {
    id: String,
    name: String,
    primary: bool,
    built_in: bool,
    profile_state: String,
    profile_id: Option<String>,
    profile_model: Option<String>,
    rendering_intent: Option<String>,
}

/// Immutable viewport diagnostics returned to the shell.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopViewportSnapshot {
    role: DesktopViewerRole,
    selected_view: DesktopViewerAnalysisView,
    presented_view: Option<DesktopViewerAnalysisView>,
    revision: u64,
    phase: String,
    physical_width: u32,
    physical_height: u32,
    surface_generation: u64,
    frame_sequence: u64,
    display_intent: String,
    display_transform: ViewerDisplayTransform,
    display_transform_id: String,
    transform_order: Vec<String>,
    profile_generation: u64,
    monitor_profiles: Vec<DesktopMonitorProfile>,
    selected_monitor_id: Option<String>,
    profile_note: String,
    summary: Option<String>,
    external_displays: Vec<DesktopExternalDisplayTarget>,
    external_output: DesktopExternalOutputSnapshot,
}

impl DesktopViewportSnapshot {
    fn for_role(role: DesktopViewerRole) -> Self {
        Self {
            role,
            selected_view: DesktopViewerAnalysisView::Image,
            presented_view: None,
            revision: 0,
            phase: "uninitialized".to_owned(),
            physical_width: 0,
            physical_height: 0,
            surface_generation: 0,
            frame_sequence: 0,
            display_intent: ViewerDisplayTransform::Srgb.intent().to_owned(),
            display_transform: ViewerDisplayTransform::Srgb,
            display_transform_id: ViewerDisplayTransform::Srgb.transform_id().to_owned(),
            transform_order: TRANSFORM_ORDER.iter().map(ToString::to_string).collect(),
            profile_generation: 0,
            monitor_profiles: Vec::new(),
            selected_monitor_id: None,
            profile_note: PROFILE_UNAVAILABLE_NOTE.to_owned(),
            summary: None,
            external_displays: Vec::new(),
            external_output: DesktopExternalOutputSnapshot::default(),
        }
    }
}

fn project_external_display_targets(
    monitors: &[DesktopMonitorDescriptor],
    main_monitor: &DesktopMonitorDescriptor,
) -> Result<Vec<DesktopExternalDisplayTarget>> {
    let main_id = main_monitor.routing_id();
    if !monitors
        .iter()
        .any(|monitor| monitor.routing_id() == main_id)
    {
        return Err(unavailable(
            "enumerate_external_displays",
            "the editor window monitor is absent from the active display catalog",
        ));
    }
    let mut targets = monitors
        .iter()
        .filter(|monitor| monitor.routing_id() != main_id)
        .map(DesktopExternalDisplayTarget::from)
        .collect::<Vec<_>>();
    targets.sort_by(|left, right| {
        (
            left.position_x,
            left.position_y,
            left.name.as_str(),
            left.id.as_str(),
        )
            .cmp(&(
                right.position_x,
                right.position_y,
                right.name.as_str(),
                right.id.as_str(),
            ))
    });
    targets.dedup_by(|left, right| left.id == right.id);
    Ok(targets)
}

fn discover_external_display_targets(
    main: &Window<Wry>,
) -> Result<Vec<DesktopExternalDisplayTarget>> {
    let primary = main
        .primary_monitor()
        .map_err(|source| native_error("read_primary_monitor", source))?;
    let primary_id = primary
        .as_ref()
        .map(|monitor| DesktopMonitorDescriptor::from_monitor(monitor, None))
        .transpose()?
        .map(|monitor| monitor.routing_id());
    let current = main
        .current_monitor()
        .map_err(|source| native_error("read_editor_monitor", source))?
        .ok_or_else(|| {
            unavailable(
                "enumerate_external_displays",
                "the editor window monitor is unavailable",
            )
        })?;
    let current = DesktopMonitorDescriptor::from_monitor(&current, primary_id.as_deref())?;
    let monitors = main
        .available_monitors()
        .map_err(|source| native_error("enumerate_external_displays", source))?
        .iter()
        .map(|monitor| DesktopMonitorDescriptor::from_monitor(monitor, primary_id.as_deref()))
        .collect::<Result<Vec<_>>>()?;
    project_external_display_targets(&monitors, &current)
}

enum ResolvedExternalDisplay {
    Active(DesktopExternalDisplayTarget),
    Inactive {
        phase: &'static str,
        summary: String,
    },
}

fn resolve_external_display(
    catalog: std::result::Result<Vec<DesktopExternalDisplayTarget>, Error>,
    requested_id: Option<&str>,
) -> (Vec<DesktopExternalDisplayTarget>, ResolvedExternalDisplay) {
    match catalog {
        Ok(targets) => {
            if let Some(requested_id) = requested_id {
                let selected = targets
                    .iter()
                    .find(|target| target.id == requested_id)
                    .cloned();
                return match selected {
                    Some(target) => (targets, ResolvedExternalDisplay::Active(target)),
                    None => (
                        targets,
                        ResolvedExternalDisplay::Inactive {
                            phase: "unavailable",
                            summary: "Selected external display is no longer available.".to_owned(),
                        },
                    ),
                };
            }
            let summary = if targets.is_empty() {
                "No external display detected."
            } else {
                "No external display selected."
            };
            (
                targets,
                ResolvedExternalDisplay::Inactive {
                    phase: "inactive",
                    summary: summary.to_owned(),
                },
            )
        }
        Err(error) => (
            Vec::new(),
            ResolvedExternalDisplay::Inactive {
                phase: "unavailable",
                summary: error.message().to_owned(),
            },
        ),
    }
}

#[derive(Clone, Debug)]
struct ViewerColorSelectionState {
    role: DesktopViewerRole,
    monitor_id: Option<MonitorId>,
    binding: Option<MonitorPresentationBinding>,
    display_transform: ViewerDisplayTransform,
}

impl ViewerColorSelectionState {
    fn new(role: DesktopViewerRole) -> Self {
        Self {
            role,
            monitor_id: None,
            binding: None,
            display_transform: ViewerDisplayTransform::Srgb,
        }
    }
}

#[derive(Clone, Debug)]
struct ViewerColorPresentation {
    binding: Option<MonitorPresentationBinding>,
    display_transform: ViewerDisplayTransform,
}
/// Serializable shell-local command failure.
#[derive(Debug, Serialize)]
pub struct DesktopViewportCommandError {
    category: String,
    recoverability: String,
    summary: String,
}

impl From<Error> for DesktopViewportCommandError {
    fn from(error: Error) -> Self {
        Self {
            category: error.category().code().to_owned(),
            recoverability: error.recoverability().code().to_owned(),
            summary: error.message().to_owned(),
        }
    }
}

enum GpuCommand {
    Present {
        role: DesktopViewerRole,
        destination: DesktopViewportSurfaceDestination,
        view: DesktopViewerAnalysisView,
        extent: ViewportExtent,
        revision: u64,
        color: ViewerColorPresentation,
        reveal_after_present: Option<Box<Window<Wry>>>,
    },
    Hidden {
        role: DesktopViewerRole,
        destination: DesktopViewportSurfaceDestination,
        revision: u64,
    },
    Shutdown,
}

struct NativeControl {
    main: Option<Window<Wry>>,
    children: Vec<(DesktopViewerRole, Window<Wry>)>,
    external_children: Vec<(DesktopViewerRole, Window<Wry>)>,
    sender: Option<mpsc::Sender<GpuCommand>>,
    worker: Option<ExecutionDomainThread<()>>,
    colors: Vec<ViewerColorSelectionState>,
    extents: Vec<(DesktopViewerRole, Option<ViewportExtent>)>,
}

/// Application-owned native viewport lifetime.
#[derive(Clone)]
pub struct DesktopViewportState {
    control: Arc<Mutex<NativeControl>>,
    snapshots: Arc<Mutex<Vec<DesktopViewportSnapshot>>>,
    profiles: Arc<Mutex<DisplayProfileCatalog>>,
    runtime: DesktopProcessRuntime,
}

impl Default for DesktopViewportState {
    fn default() -> Self {
        Self::with_runtime(DesktopProcessRuntime::new())
    }
}

impl DesktopViewportState {
    /// Creates native viewport state attached to the shared desktop process owner.
    #[must_use]
    pub fn with_runtime(runtime: DesktopProcessRuntime) -> Self {
        Self {
            control: Arc::new(Mutex::new(NativeControl {
                main: None,
                children: Vec::new(),
                external_children: Vec::new(),
                sender: None,
                worker: None,
                colors: DesktopViewerRole::ALL
                    .iter()
                    .copied()
                    .map(ViewerColorSelectionState::new)
                    .collect(),
                extents: DesktopViewerRole::ALL
                    .iter()
                    .copied()
                    .map(|role| (role, None))
                    .collect(),
            })),
            snapshots: Arc::new(Mutex::new(
                DesktopViewerRole::ALL
                    .iter()
                    .copied()
                    .map(DesktopViewportSnapshot::for_role)
                    .collect(),
            )),
            profiles: Arc::new(Mutex::new(DisplayProfileCatalog::new())),
            runtime,
        }
    }

    /// Creates inline and external native surfaces and transfers them to the GPU domain.
    pub fn initialize(&self, app: &App<Wry>) -> Result<()> {
        {
            let control = self.lock_control("initialize_native_viewport")?;
            if control.sender.is_some() || control.worker.is_some() {
                return Err(conflict(
                    "initialize_native_viewport",
                    "the native viewport is already initialized",
                ));
            }
        }
        let main = app
            .get_webview_window("main")
            .map(|window| window.as_ref().window())
            .ok_or_else(|| {
                unavailable(
                    "initialize_native_viewport",
                    "the main application window is unavailable",
                )
            })?;
        let instance = GpuInstance::new(InstanceOptions::default())?;
        let mut children = Vec::with_capacity(DesktopViewerRole::ALL.len());
        let mut external_children = Vec::with_capacity(DesktopViewerRole::ALL.len());
        let mut surfaces = Vec::with_capacity(DesktopViewerRole::ALL.len() * 2);
        for role in DesktopViewerRole::ALL {
            let child = WindowBuilder::new(app, role.window_label())
                .title(format!("Superi {} viewer", role.code()))
                .inner_size(1.0, 1.0)
                .visible(false)
                .decorations(false)
                .resizable(false)
                .focusable(false)
                .skip_taskbar(true)
                .parent(&main)
                .map_err(|source| native_error("parent_native_viewport", source))?
                .build()
                .map_err(|source| native_error("create_native_viewport", source))?;
            child
                .set_ignore_cursor_events(true)
                .map_err(|source| native_error("ignore_native_viewport_input", source))?;
            let surface = NativeViewportSurface::create(&instance, Arc::new(child.clone()))?;
            children.push((*role, child));
            surfaces.push((*role, DesktopViewportSurfaceDestination::Inline, surface));

            let external = WindowBuilder::new(app, role.external_window_label())
                .title(format!("Superi {} external viewer", role.code()))
                .inner_size(1.0, 1.0)
                .visible(false)
                .decorations(false)
                .resizable(false)
                .focusable(false)
                .skip_taskbar(true)
                .build()
                .map_err(|source| native_error("create_external_viewport", source))?;
            external
                .set_ignore_cursor_events(true)
                .map_err(|source| native_error("ignore_external_viewport_input", source))?;
            let surface = NativeViewportSurface::create(&instance, Arc::new(external.clone()))?;
            external_children.push((*role, external));
            surfaces.push((*role, DesktopViewportSurfaceDestination::External, surface));
        }

        let (profile_snapshot, profile_failure) = refresh_profile_catalog(&self.profiles);
        let mut colors = DesktopViewerRole::ALL
            .iter()
            .copied()
            .map(ViewerColorSelectionState::new)
            .collect::<Vec<_>>();
        for color in &mut colors {
            install_default_monitor(color, &profile_snapshot)?;
            publish_color_snapshot(
                &self.snapshots,
                color,
                &profile_snapshot,
                profile_failure.as_deref(),
            );
        }

        let snapshots = Arc::clone(&self.snapshots);
        let profiles = Arc::clone(&self.profiles);
        let (sender, receiver) = mpsc::channel();
        self.runtime.update_service(
            DesktopProcessServiceId::GpuSubmission,
            DesktopProcessServicePhase::Starting,
            1,
            1,
            true,
            vec![ExecutionDomain::GpuSubmission.thread_name().to_owned()],
            "Starting the GPU submission owner",
        );
        let worker_runtime = self.runtime.clone();
        let worker = ExecutionDomain::GpuSubmission
            .spawn(move |_| {
                let result = gpu_loop(instance, surfaces, receiver, &snapshots, &profiles);
                if let Err(error) = &result {
                    for role in DesktopViewerRole::ALL {
                        update_snapshot(&snapshots, *role, |state| {
                            state.phase = "failed".to_owned();
                            state.summary = Some(error.message().to_owned());
                            state.external_output.phase = "failed".to_owned();
                            state.external_output.summary = Some(error.message().to_owned());
                        });
                    }
                }
                let (phase, summary) = match &result {
                    Ok(()) => (
                        DesktopProcessServicePhase::Stopped,
                        "GPU submission owner finished and awaits join".to_owned(),
                    ),
                    Err(error) => (
                        DesktopProcessServicePhase::Failed,
                        error.message().to_owned(),
                    ),
                };
                worker_runtime.update_service(
                    DesktopProcessServiceId::GpuSubmission,
                    phase,
                    1,
                    0,
                    true,
                    vec![ExecutionDomain::GpuSubmission.thread_name().to_owned()],
                    summary,
                );
                result
            })
            .map_err(|error| {
                self.runtime.update_service(
                    DesktopProcessServiceId::GpuSubmission,
                    DesktopProcessServicePhase::Failed,
                    0,
                    0,
                    false,
                    vec![ExecutionDomain::GpuSubmission.thread_name().to_owned()],
                    error.message(),
                );
                error
            })?;

        let mut control = match self.lock_control("initialize_native_viewport") {
            Ok(control) => control,
            Err(error) => {
                let _ = sender.send(GpuCommand::Shutdown);
                let _ = worker.join();
                self.runtime.update_service(
                    DesktopProcessServiceId::GpuSubmission,
                    DesktopProcessServicePhase::Failed,
                    0,
                    0,
                    false,
                    vec![ExecutionDomain::GpuSubmission.thread_name().to_owned()],
                    error.message(),
                );
                return Err(error);
            }
        };
        if control.sender.is_some() {
            drop(control);
            let _ = sender.send(GpuCommand::Shutdown);
            let _ = worker.join();
            self.runtime.update_service(
                DesktopProcessServiceId::GpuSubmission,
                DesktopProcessServicePhase::Running,
                1,
                1,
                true,
                vec![ExecutionDomain::GpuSubmission.thread_name().to_owned()],
                "The existing GPU submission owner remains running",
            );
            return Err(conflict(
                "initialize_native_viewport",
                "the native viewport is already initialized",
            ));
        }
        control.main = Some(main);
        control.children = children;
        control.external_children = external_children;
        control.sender = Some(sender);
        control.worker = Some(worker);
        control.colors = colors;
        drop(control);
        self.runtime.update_service(
            DesktopProcessServiceId::GpuSubmission,
            DesktopProcessServicePhase::Running,
            1,
            1,
            true,
            vec![ExecutionDomain::GpuSubmission.thread_name().to_owned()],
            "GPU submission owner is running",
        );
        for role in DesktopViewerRole::ALL {
            update_snapshot(&self.snapshots, *role, |state| {
                state.phase = "initializing".to_owned();
            });
        }
        Ok(())
    }

    fn update(&self, placement: DesktopViewportPlacement) -> Result<DesktopViewportSnapshot> {
        let geometry = PhysicalViewportGeometry::from_placement(&placement)?;
        let role = placement.role;
        let (profile_snapshot, profile_failure) = refresh_profile_catalog(&self.profiles);
        let mut control = self.lock_control("update_native_viewport")?;
        let main = control.main.clone().ok_or_else(|| {
            unavailable(
                "update_native_viewport",
                "the native viewport has not initialized",
            )
        })?;
        let child = control
            .children
            .iter()
            .find_map(|(candidate, child)| (*candidate == role).then(|| child.clone()))
            .ok_or_else(|| {
                unavailable(
                    "update_native_viewport",
                    "the native viewport window is unavailable",
                )
            })?;
        let external_child = control
            .external_children
            .iter()
            .find_map(|(candidate, child)| (*candidate == role).then(|| child.clone()))
            .ok_or_else(|| {
                unavailable(
                    "update_native_viewport",
                    "the external viewport window is unavailable",
                )
            })?;
        let sender = control.sender.clone().ok_or_else(|| {
            unavailable(
                "update_native_viewport",
                "the GPU viewport owner is unavailable",
            )
        })?;
        let (external_displays, external) = resolve_external_display(
            discover_external_display_targets(&main),
            placement.external_display_id.as_deref(),
        );
        if let ResolvedExternalDisplay::Active(target) = &external {
            let snapshots = lock_snapshots(&self.snapshots, "select_external_display")?;
            if snapshots.iter().any(|snapshot| {
                snapshot.role != role
                    && snapshot.external_output.target_id.as_deref() == Some(target.id.as_str())
            }) {
                return Err(conflict(
                    "select_external_display",
                    "the external display target is already owned by another viewer",
                ));
            }
        }
        let (blocked_monitor, color_presentation, color_snapshot) = {
            let color = control
                .colors
                .iter_mut()
                .find(|color| color.role == role)
                .ok_or_else(|| {
                    unavailable("update_native_viewport", "viewer color state is missing")
                })?;
            refresh_monitor_binding(color, &profile_snapshot)?;
            (
                color.monitor_id.is_some() && color.binding.is_none(),
                ViewerColorPresentation {
                    binding: color.binding.clone(),
                    display_transform: color.display_transform,
                },
                color.clone(),
            )
        };
        let revision = {
            let mut snapshots = lock_snapshots(&self.snapshots, "update_native_viewport")?;
            let snapshot = snapshots
                .iter_mut()
                .find(|snapshot| snapshot.role == role)
                .ok_or_else(|| {
                    unavailable(
                        "update_native_viewport",
                        "the native viewer snapshot is unavailable",
                    )
                })?;
            snapshot.revision = snapshot.revision.checked_add(1).ok_or_else(|| {
                conflict(
                    "update_native_viewport",
                    "the viewport geometry revision is exhausted",
                )
            })?;
            snapshot.physical_width = geometry.width;
            snapshot.physical_height = geometry.height;
            snapshot.selected_view = placement.view;
            snapshot.phase = if placement.visible && blocked_monitor {
                "profile_selection_required"
            } else if placement.visible {
                "queued"
            } else {
                "hidden"
            }
            .to_owned();
            snapshot.summary = blocked_monitor.then(|| {
                "The selected monitor is no longer active; choose an active monitor profile"
                    .to_owned()
            });
            snapshot.external_displays = external_displays;
            snapshot.external_output.display_intent =
                color_presentation.display_transform.intent().to_owned();
            match &external {
                ResolvedExternalDisplay::Active(target) => {
                    let target_changed =
                        snapshot.external_output.target_id.as_deref() != Some(target.id.as_str());
                    snapshot.external_output.phase = if blocked_monitor {
                        "unavailable"
                    } else {
                        "queued"
                    }
                    .to_owned();
                    snapshot.external_output.target_id = Some(target.id.clone());
                    snapshot.external_output.target_name = Some(target.name.clone());
                    snapshot.external_output.selected_view = placement.view;
                    snapshot.external_output.physical_width = target.physical_width;
                    snapshot.external_output.physical_height = target.physical_height;
                    snapshot.external_output.scale_factor = target.scale_factor;
                    snapshot.external_output.summary = blocked_monitor
                        .then(|| "The selected monitor profile is no longer active.".to_owned());
                    if blocked_monitor {
                        snapshot.external_output.presented_view = None;
                    }
                    if target_changed {
                        snapshot.external_output.presented_view = None;
                        snapshot.external_output.surface_generation = 0;
                        snapshot.external_output.frame_sequence = 0;
                    }
                }
                ResolvedExternalDisplay::Inactive { phase, summary } => {
                    snapshot.external_output.phase = (*phase).to_owned();
                    snapshot.external_output.target_id = None;
                    snapshot.external_output.target_name = None;
                    snapshot.external_output.selected_view = placement.view;
                    snapshot.external_output.presented_view = None;
                    snapshot.external_output.physical_width = 0;
                    snapshot.external_output.physical_height = 0;
                    snapshot.external_output.scale_factor = 0.0;
                    snapshot.external_output.surface_generation = 0;
                    snapshot.external_output.frame_sequence = 0;
                    snapshot.external_output.summary = Some(summary.clone());
                }
            }
            snapshot.revision
        };
        publish_color_snapshot(
            &self.snapshots,
            &color_snapshot,
            &profile_snapshot,
            profile_failure.as_deref(),
        );

        let inline_extent = placement
            .visible
            .then(|| ViewportExtent::new(geometry.width, geometry.height, placement.scale_factor))
            .transpose()?;
        if let Some((_, current)) = control
            .extents
            .iter_mut()
            .find(|(candidate, _)| *candidate == role)
        {
            *current = inline_extent;
        }
        if let Some(extent) = inline_extent {
            let origin = main
                .inner_position()
                .map_err(|source| native_error("read_main_window_position", source))?;
            child
                .set_position(PhysicalPosition::new(
                    origin.x.saturating_add(geometry.x),
                    origin.y.saturating_add(geometry.y),
                ))
                .map_err(|source| native_error("position_native_viewport", source))?;
            child
                .set_size(PhysicalSize::new(geometry.width, geometry.height))
                .map_err(|source| native_error("size_native_viewport", source))?;
            if blocked_monitor {
                child
                    .hide()
                    .map_err(|source| native_error("hide_stale_profile_viewport", source))?;
                sender
                    .send(GpuCommand::Hidden {
                        role,
                        destination: DesktopViewportSurfaceDestination::Inline,
                        revision,
                    })
                    .map_err(|_| {
                        unavailable("hide_native_viewport", "the GPU viewport owner stopped")
                    })?;
            } else {
                child
                    .show()
                    .map_err(|source| native_error("show_native_viewport", source))?;
                sender
                    .send(GpuCommand::Present {
                        role,
                        destination: DesktopViewportSurfaceDestination::Inline,
                        view: placement.view,
                        extent,
                        revision,
                        color: color_presentation.clone(),
                        reveal_after_present: None,
                    })
                    .map_err(|_| {
                        unavailable(
                            "queue_native_viewport_frame",
                            "the GPU viewport owner stopped",
                        )
                    })?;
            }
        } else {
            child
                .hide()
                .map_err(|source| native_error("hide_native_viewport", source))?;
            sender
                .send(GpuCommand::Hidden {
                    role,
                    destination: DesktopViewportSurfaceDestination::Inline,
                    revision,
                })
                .map_err(|_| {
                    unavailable("hide_native_viewport", "the GPU viewport owner stopped")
                })?;
        }

        match &external {
            ResolvedExternalDisplay::Active(target) => {
                external_child
                    .set_position(PhysicalPosition::new(target.position_x, target.position_y))
                    .map_err(|source| native_error("position_external_viewport", source))?;
                external_child
                    .set_size(PhysicalSize::new(
                        target.physical_width,
                        target.physical_height,
                    ))
                    .map_err(|source| native_error("size_external_viewport", source))?;
                if blocked_monitor {
                    external_child
                        .hide()
                        .map_err(|source| native_error("hide_external_viewport", source))?;
                    sender
                        .send(GpuCommand::Hidden {
                            role,
                            destination: DesktopViewportSurfaceDestination::External,
                            revision,
                        })
                        .map_err(|_| {
                            unavailable("hide_external_viewport", "the GPU viewport owner stopped")
                        })?;
                } else {
                    external_child
                        .show()
                        .map_err(|source| native_error("show_external_viewport", source))?;
                    sender
                        .send(GpuCommand::Present {
                            role,
                            destination: DesktopViewportSurfaceDestination::External,
                            view: placement.view,
                            extent: ViewportExtent::new(
                                target.physical_width,
                                target.physical_height,
                                target.scale_factor,
                            )?,
                            revision,
                            color: color_presentation.clone(),
                            reveal_after_present: None,
                        })
                        .map_err(|_| {
                            unavailable(
                                "queue_external_viewport_frame",
                                "the GPU viewport owner stopped",
                            )
                        })?;
                }
            }
            ResolvedExternalDisplay::Inactive { .. } => {
                external_child
                    .hide()
                    .map_err(|source| native_error("hide_external_viewport", source))?;
                sender
                    .send(GpuCommand::Hidden {
                        role,
                        destination: DesktopViewportSurfaceDestination::External,
                        revision,
                    })
                    .map_err(|_| {
                        unavailable("hide_external_viewport", "the GPU viewport owner stopped")
                    })?;
            }
        }
        drop(control);
        self.snapshot(role)
    }

    fn select_color(
        &self,
        selection: DesktopViewportColorSelection,
    ) -> Result<DesktopViewportSnapshot> {
        let monitor_id = MonitorId::new(selection.monitor_id)?;
        let (profile_snapshot, profile_failure) = refresh_profile_catalog(&self.profiles);
        let binding = profile_snapshot.bind_for_presentation(&monitor_id)?;
        let role = selection.role;
        let mut control = self.lock_control("select_native_viewport_color")?;
        let sender = control.sender.clone().ok_or_else(|| {
            unavailable(
                "select_native_viewport_color",
                "the GPU viewport owner is unavailable",
            )
        })?;
        let (color_presentation, color_snapshot) = {
            let color = control
                .colors
                .iter_mut()
                .find(|color| color.role == role)
                .ok_or_else(|| {
                    unavailable(
                        "select_native_viewport_color",
                        "viewer color state is missing",
                    )
                })?;
            color.monitor_id = Some(monitor_id);
            color.binding = Some(binding);
            color.display_transform = selection.display_transform;
            (
                ViewerColorPresentation {
                    binding: color.binding.clone(),
                    display_transform: color.display_transform,
                },
                color.clone(),
            )
        };
        let prior = self.snapshot(role)?;
        let view = prior.selected_view;
        let external_extent = prior
            .external_output
            .target_id
            .as_ref()
            .map(|_| {
                ViewportExtent::new(
                    prior.external_output.physical_width,
                    prior.external_output.physical_height,
                    prior.external_output.scale_factor,
                )
            })
            .transpose()?;
        let revision =
            bump_snapshot_revision(&self.snapshots, role, "select_native_viewport_color")?;
        publish_color_snapshot(
            &self.snapshots,
            &color_snapshot,
            &profile_snapshot,
            profile_failure.as_deref(),
        );
        let inline_extent = control
            .extents
            .iter()
            .find_map(|(candidate, extent)| (*candidate == role).then_some(*extent))
            .flatten();
        update_snapshot(&self.snapshots, role, |state| {
            if inline_extent.is_some() {
                state.presented_view = None;
                state.phase = "queued".to_owned();
                state.summary = None;
            }
            state.external_output.display_intent =
                color_presentation.display_transform.intent().to_owned();
            if external_extent.is_some() {
                state.external_output.presented_view = None;
                state.external_output.phase = "queued".to_owned();
                state.external_output.summary = None;
            }
        });
        if let Some(extent) = inline_extent {
            let child = control
                .children
                .iter()
                .find_map(|(candidate, child)| (*candidate == role).then(|| child.clone()))
                .ok_or_else(|| {
                    unavailable(
                        "select_native_viewport_color",
                        "the native viewport window is unavailable",
                    )
                })?;
            child.hide().map_err(|source| {
                native_error("hide_native_viewport_during_color_selection", source)
            })?;
            sender
                .send(GpuCommand::Present {
                    role,
                    destination: DesktopViewportSurfaceDestination::Inline,
                    view,
                    extent,
                    revision,
                    color: color_presentation.clone(),
                    reveal_after_present: Some(Box::new(child)),
                })
                .map_err(|_| {
                    unavailable(
                        "queue_native_viewport_color_frame",
                        "the GPU viewport owner stopped",
                    )
                })?;
        }
        if let Some(extent) = external_extent {
            let child = control
                .external_children
                .iter()
                .find_map(|(candidate, child)| (*candidate == role).then(|| child.clone()))
                .ok_or_else(|| {
                    unavailable(
                        "select_native_viewport_color",
                        "the external viewport window is unavailable",
                    )
                })?;
            child.hide().map_err(|source| {
                native_error("hide_external_viewport_during_color_selection", source)
            })?;
            sender
                .send(GpuCommand::Present {
                    role,
                    destination: DesktopViewportSurfaceDestination::External,
                    view,
                    extent,
                    revision,
                    color: color_presentation,
                    reveal_after_present: Some(Box::new(child)),
                })
                .map_err(|_| {
                    unavailable(
                        "queue_external_viewport_color_frame",
                        "the GPU viewport owner stopped",
                    )
                })?;
        }
        drop(control);
        self.snapshot(role)
    }

    fn snapshot(&self, role: DesktopViewerRole) -> Result<DesktopViewportSnapshot> {
        lock_snapshots(&self.snapshots, "read_native_viewport")?
            .iter()
            .find(|snapshot| snapshot.role == role)
            .cloned()
            .ok_or_else(|| {
                unavailable("read_native_viewport", "the viewer snapshot is unavailable")
            })
    }

    fn lock_control(&self, operation: &'static str) -> Result<MutexGuard<'_, NativeControl>> {
        self.control.lock().map_err(|_| {
            Error::new(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "the native viewport owner lock was poisoned",
            )
            .with_context(ErrorContext::new(COMPONENT, operation))
        })
    }

    /// Stops the GPU domain, joins its retained handle, and releases native child windows.
    pub fn shutdown_and_join(&self) -> Result<()> {
        let (sender, worker, children, external_children) = {
            let mut control = self.lock_control("shutdown_and_join")?;
            (
                control.sender.take(),
                control.worker.take(),
                std::mem::take(&mut control.children),
                std::mem::take(&mut control.external_children),
            )
        };
        if sender.is_none() && worker.is_none() {
            drop(children);
            drop(external_children);
            self.runtime.update_service(
                DesktopProcessServiceId::GpuSubmission,
                DesktopProcessServicePhase::Stopped,
                0,
                0,
                false,
                vec![ExecutionDomain::GpuSubmission.thread_name().to_owned()],
                "GPU submission owner is not running",
            );
            return Ok(());
        }
        self.runtime.update_service(
            DesktopProcessServiceId::GpuSubmission,
            DesktopProcessServicePhase::Stopping,
            usize::from(worker.is_some()),
            usize::from(worker.is_some()),
            worker.is_some(),
            vec![ExecutionDomain::GpuSubmission.thread_name().to_owned()],
            "Stopping and joining the GPU submission owner",
        );
        let mut first_error = None;
        if sender.is_some_and(|sender| sender.send(GpuCommand::Shutdown).is_err()) {
            first_error = Some(unavailable(
                "shutdown_and_join",
                "the GPU submission shutdown signal could not be delivered",
            ));
        }
        if let Some(worker) = worker {
            if let Err(error) = worker.join() {
                if first_error.is_none() {
                    first_error = Some(error);
                }
            }
        }
        drop(children);
        drop(external_children);
        match first_error {
            Some(error) => {
                self.runtime.update_service(
                    DesktopProcessServiceId::GpuSubmission,
                    DesktopProcessServicePhase::Failed,
                    0,
                    0,
                    false,
                    vec![ExecutionDomain::GpuSubmission.thread_name().to_owned()],
                    error.message(),
                );
                Err(error)
            }
            None => {
                self.runtime.update_service(
                    DesktopProcessServiceId::GpuSubmission,
                    DesktopProcessServicePhase::Stopped,
                    0,
                    0,
                    false,
                    vec![ExecutionDomain::GpuSubmission.thread_name().to_owned()],
                    "GPU submission owner joined",
                );
                Ok(())
            }
        }
    }
}

impl Drop for DesktopViewportState {
    fn drop(&mut self) {
        if Arc::strong_count(&self.control) == 1 {
            let _ = self.shutdown_and_join();
        }
    }
}

/// Applies shell geometry and returns the latest viewport status.
#[tauri::command]
pub fn desktop_viewport_update(
    placement: DesktopViewportPlacement,
    state: State<'_, DesktopViewportState>,
) -> std::result::Result<DesktopViewportSnapshot, DesktopViewportCommandError> {
    state.update(placement).map_err(Into::into)
}

/// Applies an explicit monitor profile and built-in display transform selection.
#[tauri::command]
pub fn desktop_viewport_color_update(
    selection: DesktopViewportColorSelection,
    state: State<'_, DesktopViewportState>,
) -> std::result::Result<DesktopViewportSnapshot, DesktopViewportCommandError> {
    state.select_color(selection).map_err(Into::into)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PhysicalViewportGeometry {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl PhysicalViewportGeometry {
    fn from_placement(placement: &DesktopViewportPlacement) -> Result<Self> {
        if !placement.x.is_finite()
            || !placement.y.is_finite()
            || !placement.width.is_finite()
            || !placement.height.is_finite()
            || !placement.scale_factor.is_finite()
            || placement.scale_factor <= 0.0
            || placement.x < 0.0
            || placement.y < 0.0
        {
            return Err(invalid(
                "validate_native_viewport_placement",
                "viewport geometry and scale must be finite and nonnegative",
            ));
        }
        if !placement.visible {
            return Ok(Self {
                x: 0,
                y: 0,
                width: 0,
                height: 0,
            });
        }
        if placement.width <= 0.0 || placement.height <= 0.0 {
            return Err(invalid(
                "validate_native_viewport_placement",
                "a visible viewport requires positive dimensions",
            ));
        }

        let x = physical_i32(placement.x, placement.scale_factor)?;
        let y = physical_i32(placement.y, placement.scale_factor)?;
        let width = physical_u32(placement.width, placement.scale_factor)?;
        let height = physical_u32(placement.height, placement.scale_factor)?;
        Ok(Self {
            x,
            y,
            width,
            height,
        })
    }
}

fn gpu_loop(
    instance: GpuInstance,
    mut surfaces: Vec<(
        DesktopViewerRole,
        DesktopViewportSurfaceDestination,
        NativeViewportSurface,
    )>,
    receiver: mpsc::Receiver<GpuCommand>,
    snapshots: &Arc<Mutex<Vec<DesktopViewportSnapshot>>>,
    profiles: &Arc<Mutex<DisplayProfileCatalog>>,
) -> Result<()> {
    let first_surface = surfaces.first().ok_or_else(|| {
        unavailable(
            "initialize_native_viewers",
            "at least one native viewer surface is required",
        )
    })?;
    let adapter = first_surface
        .2
        .compatible_adapters(&instance)?
        .select(&AdapterSelection::default())?;
    let device = pollster::block_on(
        adapter.create_device(&DeviceRequest::default().with_label("Superi native viewport")),
    )?;
    let resources = GpuResources::new(&device)?;
    let submissions = GpuSubmissionQueue::new(&device)?;
    let render_results = create_initial_render_results(&resources, &submissions)?;
    for role in DesktopViewerRole::ALL {
        update_snapshot(snapshots, *role, |state| {
            state.phase = "ready".to_owned();
        });
    }

    while let Ok(command) = receiver.recv() {
        match command {
            GpuCommand::Present {
                role,
                destination,
                view,
                extent,
                revision,
                color,
                reveal_after_present,
            } => {
                if !snapshot_revision_is_current(
                    snapshots,
                    role,
                    revision,
                    "prepare_native_viewer_frame",
                )? {
                    continue;
                }
                let surface = surfaces
                    .iter_mut()
                    .find_map(|(candidate_role, candidate_destination, surface)| {
                        (*candidate_role == role && *candidate_destination == destination)
                            .then_some(surface)
                    })
                    .ok_or_else(|| {
                        unavailable("present_native_viewer", "the viewer surface is unavailable")
                    })?;
                let render_result = render_results
                    .iter()
                    .find(|result| result.intent.role() == role)
                    .ok_or_else(|| {
                        unavailable(
                            "present_native_viewer",
                            "the GPU render result is unavailable",
                        )
                    })?;
                let presented = present_once(
                    surface,
                    &device,
                    &resources,
                    &submissions,
                    render_result,
                    view,
                    extent,
                    &color,
                    profiles,
                );
                match presented {
                    Ok((generation, sequence, intent)) => {
                        let mut states = lock_snapshots(snapshots, "publish_native_viewer_frame")?;
                        let state = states
                            .iter_mut()
                            .find(|state| state.role == role)
                            .ok_or_else(|| {
                                unavailable(
                                    "publish_native_viewer_frame",
                                    "the native viewer snapshot is unavailable",
                                )
                            })?;
                        if state.revision != revision {
                            continue;
                        }
                        if let Some(child) = reveal_after_present {
                            child.show().map_err(|source| {
                                native_error(
                                    match destination {
                                        DesktopViewportSurfaceDestination::Inline => {
                                            "show_native_viewport_after_color_selection"
                                        }
                                        DesktopViewportSurfaceDestination::External => {
                                            "show_external_viewport_after_color_selection"
                                        }
                                    },
                                    source,
                                )
                            })?;
                        }
                        match destination {
                            DesktopViewportSurfaceDestination::Inline => {
                                state.phase = "presenting".to_owned();
                                state.presented_view = Some(view);
                                state.surface_generation = generation;
                                state.frame_sequence = sequence;
                                state.display_intent = intent.display_intent().to_owned();
                                state.display_transform = color.display_transform;
                                state.display_transform_id =
                                    intent.display_transform_id().to_owned();
                                state.summary = None;
                            }
                            DesktopViewportSurfaceDestination::External => {
                                state.external_output.phase = "presenting".to_owned();
                                state.external_output.presented_view = Some(view);
                                state.external_output.surface_generation = generation;
                                state.external_output.frame_sequence = sequence;
                                state.external_output.display_intent =
                                    intent.display_intent().to_owned();
                                state.external_output.summary = None;
                            }
                        }
                    }
                    Err(error) if destination == DesktopViewportSurfaceDestination::External => {
                        update_snapshot(snapshots, role, |state| {
                            if state.revision == revision {
                                state.external_output.phase = "failed".to_owned();
                                state.external_output.presented_view = None;
                                state.external_output.summary = Some(error.message().to_owned());
                            }
                        });
                        continue;
                    }
                    Err(error) if error.recoverability() != Recoverability::Terminal => {
                        update_snapshot(snapshots, role, |state| {
                            if state.revision == revision {
                                state.phase = "profile_or_surface_retry".to_owned();
                                state.summary = Some(error.message().to_owned());
                            }
                        });
                    }
                    Err(error) => return Err(error),
                }
            }
            GpuCommand::Hidden {
                role,
                destination,
                revision,
            } => {
                update_snapshot(snapshots, role, |state| {
                    if state.revision == revision {
                        match destination {
                            DesktopViewportSurfaceDestination::Inline => {
                                state.phase = "hidden".to_owned();
                                state.presented_view = None;
                            }
                            DesktopViewportSurfaceDestination::External => {
                                state.external_output.presented_view = None;
                            }
                        }
                    }
                });
            }
            GpuCommand::Shutdown => break,
        }
    }
    Ok(())
}

fn create_initial_render_results(
    resources: &GpuResources<'_>,
    submissions: &GpuSubmissionQueue<'_>,
) -> Result<Vec<ViewerGpuRenderResult>> {
    let extent = wgpu::Extent3d {
        width: 16,
        height: 9,
        depth_or_array_layers: 1,
    };
    let mut results = Vec::with_capacity(DesktopViewerRole::ALL.len());
    for role in DesktopViewerRole::ALL {
        let texture = resources.create_texture(&wgpu::TextureDescriptor {
            label: Some(role.source_label()),
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba16Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        })?;
        clear_source(resources, submissions, &texture, role.clear_color())?;
        results.push(ViewerGpuRenderResult::new(
            ViewerPresentationIntent::canonical(*role, extent)?,
            texture,
        )?);
    }
    Ok(results)
}

fn clear_source(
    resources: &GpuResources<'_>,
    submissions: &GpuSubmissionQueue<'_>,
    source: &superi_gpu::texture::GpuTexture,
    color: wgpu::Color,
) -> Result<()> {
    let view = source
        .raw()
        .create_view(&wgpu::TextureViewDescriptor::default());
    let mut encoder =
        resources
            .device()
            .wgpu_device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("clear native viewport source"),
            });
    {
        let _pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("clear native viewport source"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(color),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });
    }
    let mut retained = submissions.resources();
    retained.retain(source.clone());
    let fence = submissions.submit([encoder.finish()], retained)?;
    let _ = submissions.wait(&fence)?;
    Ok(())
}

fn present_once<'device>(
    surface: &mut NativeViewportSurface,
    device: &'device superi_gpu::device::GpuDevice,
    resources: &GpuResources<'device>,
    submissions: &GpuSubmissionQueue<'device>,
    render_result: &ViewerGpuRenderResult,
    view: DesktopViewerAnalysisView,
    extent: ViewportExtent,
    color: &ViewerColorPresentation,
    profiles: &Arc<Mutex<DisplayProfileCatalog>>,
) -> Result<(u64, u64, ViewerPresentationIntent)> {
    ensure_profile_current(
        color.binding.as_ref(),
        profiles,
        "prepare_native_viewer_frame",
    )?;
    let intent = ViewerPresentationIntent::for_display(
        render_result.intent.role(),
        render_result.intent.source_extent(),
        color.display_transform,
    )?;
    let configuration = surface.configure(device, extent)?.clone();
    let presenter = GpuDisplayPresenter::new_with_view(
        resources,
        intent.display_transform(),
        configuration.format,
        view.gpu_view(),
    )?;
    let prepared = presenter.prepare_source(render_result.texture.clone())?;
    let frame = surface.acquire_frame(device)?;
    let generation = frame.generation();
    let sequence = frame.sequence();
    let target = frame
        .texture()
        .create_view(&wgpu::TextureViewDescriptor::default());
    let encoded = presenter.encode(
        &prepared,
        &target,
        wgpu::Extent3d {
            width: configuration.width,
            height: configuration.height,
            depth_or_array_layers: 1,
        },
    )?;
    ensure_profile_current(
        color.binding.as_ref(),
        profiles,
        "present_native_viewer_frame",
    )?;
    let fence = encoded.submit_and_present(frame, submissions)?;
    let _ = submissions.wait(&fence)?;
    Ok((generation, sequence, intent))
}

fn refresh_profile_catalog(
    profiles: &Arc<Mutex<DisplayProfileCatalog>>,
) -> (DisplayProfileSnapshot, Option<String>) {
    let Ok(mut catalog) = profiles.lock() else {
        return (
            DisplayProfileSnapshot::default(),
            Some("the monitor profile catalog lock was poisoned".to_owned()),
        );
    };
    let failure = refresh_system_profile_catalog(&mut catalog)
        .err()
        .map(|error| error.message().to_owned());
    (catalog.snapshot(), failure)
}

#[cfg(target_os = "macos")]
fn refresh_system_profile_catalog(catalog: &mut DisplayProfileCatalog) -> Result<()> {
    let _ = catalog.refresh(&SystemDisplayProfileDiscovery)?;
    Ok(())
}

#[cfg(not(target_os = "macos"))]
fn refresh_system_profile_catalog(_catalog: &mut DisplayProfileCatalog) -> Result<()> {
    Ok(())
}

fn install_default_monitor(
    color: &mut ViewerColorSelectionState,
    snapshot: &DisplayProfileSnapshot,
) -> Result<()> {
    let Some(display) = snapshot
        .primary_display()
        .or_else(|| snapshot.displays().first())
    else {
        color.monitor_id = None;
        color.binding = None;
        return Ok(());
    };
    color.monitor_id = Some(display.id().clone());
    color.binding = Some(snapshot.bind_for_presentation(display.id())?);
    Ok(())
}

fn refresh_monitor_binding(
    color: &mut ViewerColorSelectionState,
    snapshot: &DisplayProfileSnapshot,
) -> Result<()> {
    let Some(monitor_id) = color.monitor_id.as_ref() else {
        return install_default_monitor(color, snapshot);
    };
    color.binding = snapshot.bind_for_presentation(monitor_id).ok();
    Ok(())
}

fn publish_color_snapshot(
    snapshots: &Arc<Mutex<Vec<DesktopViewportSnapshot>>>,
    color: &ViewerColorSelectionState,
    profile_snapshot: &DisplayProfileSnapshot,
    profile_failure: Option<&str>,
) {
    let monitor_profiles = profile_snapshot
        .displays()
        .iter()
        .map(desktop_monitor_profile)
        .collect::<Vec<_>>();
    let profile_note = profile_failure.map_or_else(
        || match color.binding.as_ref() {
            Some(binding) => match binding.state() {
                PresentationProfileState::Profiled { .. } => PROFILE_IDENTITY_NOTE.to_owned(),
                PresentationProfileState::Unprofiled => UNPROFILED_NOTE.to_owned(),
            },
            None => color.monitor_id.as_ref().map_or_else(
                || PROFILE_UNAVAILABLE_NOTE.to_owned(),
                |monitor_id| {
                    format!(
                        "The selected monitor {} is no longer active; choose an active monitor profile.",
                        monitor_id.as_str()
                    )
                },
            ),
        },
        |failure| format!("Monitor profile refresh failed: {failure}"),
    );
    update_snapshot(snapshots, color.role, |state| {
        state.profile_generation = profile_snapshot.generation();
        state.monitor_profiles = monitor_profiles;
        state.selected_monitor_id = color
            .binding
            .as_ref()
            .map(|binding| binding.monitor_id().as_str().to_owned());
        state.display_transform = color.display_transform;
        state.display_intent = color.display_transform.intent().to_owned();
        state.display_transform_id = color.display_transform.transform_id().to_owned();
        state.transform_order = TRANSFORM_ORDER.iter().map(ToString::to_string).collect();
        state.profile_note = profile_note;
    });
}

fn desktop_monitor_profile(display: &superi_color::icc::DisplayProfile) -> DesktopMonitorProfile {
    let (profile_state, profile_id, profile_model, rendering_intent) =
        display.profile().map_or_else(
            || ("unprofiled".to_owned(), None, None, None),
            |profile| {
                (
                    "profiled".to_owned(),
                    Some(profile.id().to_string()),
                    Some(icc_model_code(profile.display_model()).to_owned()),
                    Some(icc_rendering_intent_code(profile.rendering_intent()).to_owned()),
                )
            },
        );
    DesktopMonitorProfile {
        id: display.id().as_str().to_owned(),
        name: display.name().to_owned(),
        primary: display.is_primary(),
        built_in: display.is_built_in(),
        profile_state,
        profile_id,
        profile_model,
        rendering_intent,
    }
}

fn icc_model_code(model: IccDisplayModel) -> &'static str {
    match model {
        IccDisplayModel::MatrixTrc => "matrix_trc",
        IccDisplayModel::Lut => "lut",
        _ => "unknown",
    }
}

fn icc_rendering_intent_code(intent: IccRenderingIntent) -> &'static str {
    match intent {
        IccRenderingIntent::Perceptual => "perceptual",
        IccRenderingIntent::MediaRelativeColorimetric => "media_relative_colorimetric",
        IccRenderingIntent::Saturation => "saturation",
        IccRenderingIntent::AbsoluteColorimetric => "absolute_colorimetric",
        _ => "unknown",
    }
}

fn ensure_profile_current(
    binding: Option<&MonitorPresentationBinding>,
    profiles: &Arc<Mutex<DisplayProfileCatalog>>,
    operation: &'static str,
) -> Result<()> {
    let Some(binding) = binding else {
        return Ok(());
    };
    let catalog = profiles.lock().map_err(|_| {
        Error::new(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "the monitor profile catalog lock was poisoned",
        )
        .with_context(ErrorContext::new(COMPONENT, operation))
    })?;
    let snapshot = catalog.snapshot();
    if binding.is_current(&snapshot) {
        return Ok(());
    }
    Err(Error::new(
        ErrorCategory::Conflict,
        Recoverability::Retryable,
        "the selected monitor profile changed before native presentation",
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("monitor_id", binding.monitor_id().as_str())
            .with_field(
                "binding_generation",
                binding.catalog_generation().to_string(),
            )
            .with_field("profile_generation", snapshot.generation().to_string()),
    ))
}

fn bump_snapshot_revision(
    snapshots: &Arc<Mutex<Vec<DesktopViewportSnapshot>>>,
    role: DesktopViewerRole,
    operation: &'static str,
) -> Result<u64> {
    let mut snapshots = lock_snapshots(snapshots, operation)?;
    let snapshot = snapshots
        .iter_mut()
        .find(|snapshot| snapshot.role == role)
        .ok_or_else(|| unavailable(operation, "the native viewer snapshot is unavailable"))?;
    snapshot.revision = snapshot
        .revision
        .checked_add(1)
        .ok_or_else(|| conflict(operation, "the viewport revision is exhausted"))?;
    Ok(snapshot.revision)
}

fn snapshot_revision_is_current(
    snapshots: &Arc<Mutex<Vec<DesktopViewportSnapshot>>>,
    role: DesktopViewerRole,
    revision: u64,
    operation: &'static str,
) -> Result<bool> {
    let snapshots = lock_snapshots(snapshots, operation)?;
    let snapshot = snapshots
        .iter()
        .find(|snapshot| snapshot.role == role)
        .ok_or_else(|| unavailable(operation, "the native viewer snapshot is unavailable"))?;
    Ok(snapshot.revision == revision)
}

fn update_snapshot(
    snapshots: &Arc<Mutex<Vec<DesktopViewportSnapshot>>>,
    role: DesktopViewerRole,
    update: impl FnOnce(&mut DesktopViewportSnapshot),
) {
    if let Ok(mut snapshots) = snapshots.lock() {
        if let Some(snapshot) = snapshots.iter_mut().find(|snapshot| snapshot.role == role) {
            update(snapshot);
        }
    }
}

fn lock_snapshots<'a>(
    snapshots: &'a Arc<Mutex<Vec<DesktopViewportSnapshot>>>,
    operation: &'static str,
) -> Result<MutexGuard<'a, Vec<DesktopViewportSnapshot>>> {
    snapshots.lock().map_err(|_| {
        Error::new(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "the native viewport snapshot lock was poisoned",
        )
        .with_context(ErrorContext::new(COMPONENT, operation))
    })
}

fn physical_i32(logical: f64, scale: f64) -> Result<i32> {
    let value = (logical * scale).round();
    if value < f64::from(i32::MIN) || value > f64::from(i32::MAX) {
        return Err(invalid(
            "convert_native_viewport_geometry",
            "viewport position exceeds native window limits",
        ));
    }
    Ok(value as i32)
}

fn physical_u32(logical: f64, scale: f64) -> Result<u32> {
    let value = (logical * scale).round();
    if value < 1.0 || value > f64::from(u32::MAX) {
        return Err(invalid(
            "convert_native_viewport_geometry",
            "viewport extent exceeds native surface limits",
        ));
    }
    Ok(value as u32)
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

fn unavailable(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

fn native_error(operation: &'static str, source: tauri::Error) -> Error {
    Error::with_source(
        ErrorCategory::Unavailable,
        Recoverability::Retryable,
        "native viewport window operation failed",
        source,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::{
        ensure_profile_current, install_default_monitor, project_external_display_targets,
        publish_color_snapshot, refresh_monitor_binding, snapshot_revision_is_current,
        DesktopMonitorDescriptor, DesktopViewerAnalysisView, DesktopViewerRole,
        DesktopViewportPlacement, DesktopViewportSnapshot, PhysicalViewportGeometry,
        ViewerColorSelectionState,
    };
    use superi_color::icc::{
        DisplayProfileCatalog, DisplayProfileObservation, MonitorId, NativeDisplayProfileProvider,
    };

    #[test]
    fn placement_converts_css_geometry_to_physical_pixels() {
        let geometry = PhysicalViewportGeometry::from_placement(&DesktopViewportPlacement {
            role: DesktopViewerRole::Program,
            view: DesktopViewerAnalysisView::Luminance,
            x: 10.25,
            y: 20.5,
            width: 960.0,
            height: 540.0,
            scale_factor: 2.0,
            visible: true,
            external_display_id: None,
        })
        .unwrap();

        assert_eq!(geometry.x, 21);
        assert_eq!(geometry.y, 41);
        assert_eq!(geometry.width, 1_920);
        assert_eq!(geometry.height, 1_080);
    }

    #[test]
    fn hidden_placement_accepts_zero_extent_without_surface_configuration() {
        let geometry = PhysicalViewportGeometry::from_placement(&DesktopViewportPlacement {
            role: DesktopViewerRole::Program,
            view: DesktopViewerAnalysisView::Clipping,
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            scale_factor: 2.0,
            visible: false,
            external_display_id: None,
        })
        .unwrap();

        assert_eq!(geometry.width, 0);
        assert_eq!(geometry.height, 0);
    }

    #[test]
    fn external_catalog_excludes_the_editor_monitor_and_preserves_exact_geometry() {
        let main =
            DesktopMonitorDescriptor::new(Some("Editor".to_owned()), 0, 0, 2560, 1440, 2.0, true)
                .unwrap();
        let studio = DesktopMonitorDescriptor::new(
            Some("Studio".to_owned()),
            2560,
            -120,
            3840,
            2160,
            2.0,
            false,
        )
        .unwrap();
        let client = DesktopMonitorDescriptor::new(
            Some("Client".to_owned()),
            -1920,
            0,
            1920,
            1080,
            1.0,
            false,
        )
        .unwrap();

        let targets = project_external_display_targets(
            &[main.clone(), studio.clone(), client.clone()],
            &main,
        )
        .unwrap();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[0].name, "Client");
        assert_eq!(targets[0].position_x, -1920);
        assert_eq!(targets[0].physical_width, 1920);
        assert_eq!(targets[1].name, "Studio");
        assert_eq!(targets[1].position_y, -120);
        assert_eq!(targets[1].physical_width, 3840);
        assert_eq!(targets[1].physical_height, 2160);
        assert_eq!(targets[1].scale_factor, 2.0);
        assert_ne!(targets[0].id, targets[1].id);
        assert_eq!(
            project_external_display_targets(&[main], &studio)
                .unwrap_err()
                .category(),
            superi_core::error::ErrorCategory::Unavailable,
        );
    }

    #[test]
    fn monitor_selection_is_exact_reversible_and_stale_state_fails_closed() {
        let primary = DisplayProfileObservation::new(
            MonitorId::new("display:primary").unwrap(),
            "Primary",
            true,
            true,
            None,
        )
        .unwrap();
        let reference = DisplayProfileObservation::new(
            MonitorId::new("display:reference").unwrap(),
            "Reference",
            false,
            false,
            None,
        )
        .unwrap();
        let profiles = Arc::new(Mutex::new(DisplayProfileCatalog::new()));
        profiles
            .lock()
            .unwrap()
            .refresh(
                &NativeDisplayProfileProvider::new(vec![primary.clone(), reference.clone()])
                    .unwrap(),
            )
            .unwrap();
        let snapshot = profiles.lock().unwrap().snapshot();
        let mut color = ViewerColorSelectionState::new(DesktopViewerRole::Program);
        install_default_monitor(&mut color, &snapshot).unwrap();
        assert_eq!(
            color.monitor_id.as_ref().unwrap().as_str(),
            "display:primary"
        );
        assert!(color.binding.as_ref().unwrap().is_current(&snapshot));

        let reference_id = MonitorId::new("display:reference").unwrap();
        color.monitor_id = Some(reference_id.clone());
        color.binding = Some(snapshot.bind_for_presentation(&reference_id).unwrap());
        let captured = color.binding.clone().unwrap();
        ensure_profile_current(Some(&captured), &profiles, "test_profile_before_frame").unwrap();

        profiles
            .lock()
            .unwrap()
            .refresh(&NativeDisplayProfileProvider::new(vec![primary]).unwrap())
            .unwrap();
        let refreshed = profiles.lock().unwrap().snapshot();
        assert!(
            ensure_profile_current(Some(&captured), &profiles, "test_profile_before_present")
                .is_err()
        );
        refresh_monitor_binding(&mut color, &refreshed).unwrap();
        assert_eq!(color.monitor_id.as_ref(), Some(&reference_id));
        assert!(color.binding.is_none());
        let snapshots = Arc::new(Mutex::new(vec![DesktopViewportSnapshot::for_role(
            DesktopViewerRole::Program,
        )]));
        assert!(snapshot_revision_is_current(
            &snapshots,
            DesktopViewerRole::Program,
            0,
            "test_current_color_revision",
        )
        .unwrap());
        assert!(!snapshot_revision_is_current(
            &snapshots,
            DesktopViewerRole::Program,
            1,
            "test_stale_color_revision",
        )
        .unwrap());
        publish_color_snapshot(&snapshots, &color, &refreshed, None);
        let published = snapshots.lock().unwrap()[0].clone();
        assert_eq!(published.selected_monitor_id, None);
        assert!(published.profile_note.contains("is no longer active"));

        color.monitor_id = None;
        install_default_monitor(&mut color, &refreshed).unwrap();
        assert_eq!(
            color.monitor_id.as_ref().unwrap().as_str(),
            "display:primary"
        );
    }
}
