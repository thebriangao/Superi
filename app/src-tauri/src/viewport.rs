//! Native viewport window ownership and GPU-resident presentation.

use std::sync::{mpsc, Arc, Mutex, MutexGuard};

use serde::{Deserialize, Serialize};
use superi_color::gpu_display::GpuDisplayPresenter;
use superi_color::transform_out::{OutputColorTransform, OutputTargetKind, OutputTransformOptions};
use superi_color::working_space::WorkingSpace;
use superi_concurrency::threads::{ExecutionDomain, ExecutionDomainThread};
use superi_core::color_space::ColorSpace;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_gpu::device::{AdapterSelection, DeviceRequest, GpuInstance, InstanceOptions};
use superi_gpu::resource::GpuResources;
use superi_gpu::submission::GpuSubmissionQueue;
use superi_gpu::surface::{NativeViewportSurface, ViewportExtent};
use superi_gpu::wgpu;
use tauri::window::WindowBuilder;
use tauri::{App, Manager, PhysicalPosition, PhysicalSize, State, Window, Wry};

const VIEWPORT_LABEL: &str = "native-media-viewport";
const COMPONENT: &str = "superi-desktop.viewport";

/// Geometry and visibility published by the React workspace shell.
#[derive(Clone, Copy, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopViewportPlacement {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
    scale_factor: f64,
    visible: bool,
}

/// Immutable viewport diagnostics returned to the shell.
#[derive(Clone, Debug, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct DesktopViewportSnapshot {
    revision: u64,
    phase: String,
    physical_width: u32,
    physical_height: u32,
    surface_generation: u64,
    frame_sequence: u64,
    summary: Option<String>,
}

impl Default for DesktopViewportSnapshot {
    fn default() -> Self {
        Self {
            revision: 0,
            phase: "uninitialized".to_owned(),
            physical_width: 0,
            physical_height: 0,
            surface_generation: 0,
            frame_sequence: 0,
            summary: None,
        }
    }
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
        extent: ViewportExtent,
        revision: u64,
    },
    Hidden {
        revision: u64,
    },
    Shutdown,
}

struct NativeControl {
    main: Option<Window<Wry>>,
    child: Option<Window<Wry>>,
    sender: Option<mpsc::Sender<GpuCommand>>,
    worker: Option<ExecutionDomainThread<()>>,
}

/// Application-owned native viewport lifetime.
pub struct DesktopViewportState {
    control: Mutex<NativeControl>,
    snapshot: Arc<Mutex<DesktopViewportSnapshot>>,
}

impl Default for DesktopViewportState {
    fn default() -> Self {
        Self {
            control: Mutex::new(NativeControl {
                main: None,
                child: None,
                sender: None,
                worker: None,
            }),
            snapshot: Arc::new(Mutex::new(DesktopViewportSnapshot::default())),
        }
    }
}

impl DesktopViewportState {
    /// Creates the native child and transfers presentation ownership to the GPU domain.
    pub fn initialize(&self, app: &App<Wry>) -> Result<()> {
        let main = app
            .get_webview_window("main")
            .map(|window| window.as_ref().window())
            .ok_or_else(|| {
                unavailable(
                    "initialize_native_viewport",
                    "the main application window is unavailable",
                )
            })?;
        let child = WindowBuilder::new(app, VIEWPORT_LABEL)
            .title("Superi native media viewport")
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

        let instance = GpuInstance::new(InstanceOptions::default())?;
        let surface = NativeViewportSurface::create(&instance, Arc::new(child.clone()))?;
        let snapshot = Arc::clone(&self.snapshot);
        let (sender, receiver) = mpsc::channel();
        let worker = ExecutionDomain::GpuSubmission.spawn(move |_| {
            let result = gpu_loop(instance, surface, receiver, &snapshot);
            if let Err(error) = &result {
                update_snapshot(&snapshot, |state| {
                    state.phase = "failed".to_owned();
                    state.summary = Some(error.message().to_owned());
                });
            }
            result
        })?;

        let mut control = self.lock_control("initialize_native_viewport")?;
        if control.sender.is_some() {
            return Err(conflict(
                "initialize_native_viewport",
                "the native viewport is already initialized",
            ));
        }
        control.main = Some(main);
        control.child = Some(child);
        control.sender = Some(sender);
        control.worker = Some(worker);
        drop(control);
        update_snapshot(&self.snapshot, |state| {
            state.phase = "initializing".to_owned();
        });
        Ok(())
    }

    fn update(&self, placement: DesktopViewportPlacement) -> Result<DesktopViewportSnapshot> {
        let geometry = PhysicalViewportGeometry::from_placement(placement)?;
        let control = self.lock_control("update_native_viewport")?;
        let main = control.main.as_ref().ok_or_else(|| {
            unavailable(
                "update_native_viewport",
                "the native viewport has not initialized",
            )
        })?;
        let child = control.child.as_ref().ok_or_else(|| {
            unavailable(
                "update_native_viewport",
                "the native viewport window is unavailable",
            )
        })?;
        let sender = control.sender.as_ref().ok_or_else(|| {
            unavailable(
                "update_native_viewport",
                "the GPU viewport owner is unavailable",
            )
        })?;

        let revision = {
            let mut snapshot = lock_snapshot(&self.snapshot, "update_native_viewport")?;
            snapshot.revision = snapshot.revision.checked_add(1).ok_or_else(|| {
                conflict(
                    "update_native_viewport",
                    "the viewport geometry revision is exhausted",
                )
            })?;
            snapshot.physical_width = geometry.width;
            snapshot.physical_height = geometry.height;
            snapshot.summary = None;
            snapshot.revision
        };

        if placement.visible {
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
            child
                .show()
                .map_err(|source| native_error("show_native_viewport", source))?;
            sender
                .send(GpuCommand::Present {
                    extent: ViewportExtent::new(
                        geometry.width,
                        geometry.height,
                        placement.scale_factor,
                    )?,
                    revision,
                })
                .map_err(|_| {
                    unavailable(
                        "queue_native_viewport_frame",
                        "the GPU viewport owner stopped",
                    )
                })?;
            update_snapshot(&self.snapshot, |state| {
                state.phase = "queued".to_owned();
            });
        } else {
            child
                .hide()
                .map_err(|source| native_error("hide_native_viewport", source))?;
            sender.send(GpuCommand::Hidden { revision }).map_err(|_| {
                unavailable("hide_native_viewport", "the GPU viewport owner stopped")
            })?;
            update_snapshot(&self.snapshot, |state| {
                state.phase = "hidden".to_owned();
            });
        }
        drop(control);
        self.snapshot()
    }

    fn snapshot(&self) -> Result<DesktopViewportSnapshot> {
        Ok(lock_snapshot(&self.snapshot, "read_native_viewport")?.clone())
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
}

impl Drop for DesktopViewportState {
    fn drop(&mut self) {
        let (worker, child) = match self.control.get_mut() {
            Ok(control) => {
                if let Some(sender) = control.sender.take() {
                    let _ = sender.send(GpuCommand::Shutdown);
                }
                (control.worker.take(), control.child.take())
            }
            Err(_) => (None, None),
        };
        if let Some(worker) = worker {
            let _ = worker.join();
        }
        drop(child);
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

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct PhysicalViewportGeometry {
    x: i32,
    y: i32,
    width: u32,
    height: u32,
}

impl PhysicalViewportGeometry {
    fn from_placement(placement: DesktopViewportPlacement) -> Result<Self> {
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
    mut surface: NativeViewportSurface,
    receiver: mpsc::Receiver<GpuCommand>,
    snapshot: &Arc<Mutex<DesktopViewportSnapshot>>,
) -> Result<()> {
    let adapter = surface
        .compatible_adapters(&instance)?
        .select(&AdapterSelection::default())?;
    let device = pollster::block_on(
        adapter.create_device(&DeviceRequest::default().with_label("Superi native viewport")),
    )?;
    let resources = GpuResources::new(&device)?;
    let submissions = GpuSubmissionQueue::new(&device)?;
    let source = resources.create_texture(&wgpu::TextureDescriptor {
        label: Some("native viewport canonical source"),
        size: wgpu::Extent3d {
            width: 16,
            height: 9,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })?;
    clear_source(&resources, &submissions, &source)?;
    update_snapshot(snapshot, |state| {
        state.phase = "ready".to_owned();
    });

    while let Ok(command) = receiver.recv() {
        match command {
            GpuCommand::Present { extent, revision } => {
                let (generation, sequence) = present_once(
                    &mut surface,
                    &device,
                    &resources,
                    &submissions,
                    &source,
                    extent,
                )?;
                update_snapshot(snapshot, |state| {
                    state.revision = state.revision.max(revision);
                    state.phase = "presenting".to_owned();
                    state.surface_generation = generation;
                    state.frame_sequence = sequence;
                    state.summary = None;
                });
            }
            GpuCommand::Hidden { revision } => {
                update_snapshot(snapshot, |state| {
                    state.revision = state.revision.max(revision);
                    state.phase = "hidden".to_owned();
                });
            }
            GpuCommand::Shutdown => break,
        }
    }
    Ok(())
}

fn clear_source(
    resources: &GpuResources<'_>,
    submissions: &GpuSubmissionQueue<'_>,
    source: &superi_gpu::texture::GpuTexture,
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
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: 0.18,
                        g: 0.035,
                        b: 0.012,
                        a: 1.0,
                    }),
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
    source: &superi_gpu::texture::GpuTexture,
    extent: ViewportExtent,
) -> Result<(u64, u64)> {
    let configuration = surface.configure(device, extent)?.clone();
    let reference = OutputColorTransform::new(
        OutputTargetKind::Display,
        WorkingSpace::ACESCG,
        ColorSpace::SRGB,
        OutputTransformOptions::new(),
    )?;
    let presenter = GpuDisplayPresenter::new(resources, reference, configuration.format)?;
    let prepared = presenter.prepare_source(source.clone())?;
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
    let fence = encoded.submit_and_present(frame, submissions)?;
    let _ = submissions.wait(&fence)?;
    Ok((generation, sequence))
}

fn update_snapshot(
    snapshot: &Arc<Mutex<DesktopViewportSnapshot>>,
    update: impl FnOnce(&mut DesktopViewportSnapshot),
) {
    if let Ok(mut snapshot) = snapshot.lock() {
        update(&mut snapshot);
    }
}

fn lock_snapshot<'a>(
    snapshot: &'a Arc<Mutex<DesktopViewportSnapshot>>,
    operation: &'static str,
) -> Result<MutexGuard<'a, DesktopViewportSnapshot>> {
    snapshot.lock().map_err(|_| {
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
    use super::{DesktopViewportPlacement, PhysicalViewportGeometry};

    #[test]
    fn placement_converts_css_geometry_to_physical_pixels() {
        let geometry = PhysicalViewportGeometry::from_placement(DesktopViewportPlacement {
            x: 10.25,
            y: 20.5,
            width: 960.0,
            height: 540.0,
            scale_factor: 2.0,
            visible: true,
        })
        .unwrap();

        assert_eq!(geometry.x, 21);
        assert_eq!(geometry.y, 41);
        assert_eq!(geometry.width, 1_920);
        assert_eq!(geometry.height, 1_080);
    }

    #[test]
    fn hidden_placement_accepts_zero_extent_without_surface_configuration() {
        let geometry = PhysicalViewportGeometry::from_placement(DesktopViewportPlacement {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
            scale_factor: 2.0,
            visible: false,
        })
        .unwrap();

        assert_eq!(geometry.width, 0);
        assert_eq!(geometry.height, 0);
    }
}
