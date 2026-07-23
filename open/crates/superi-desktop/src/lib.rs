#![forbid(unsafe_code)]

//! Thin winit host for the shared Superi retained scene and wgpu compositor.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use accesskit::{Action as AccessAction, ActionRequest, NodeId as AccessNodeId};
use accesskit_winit::{
    Adapter as AccessibilityAdapter, Event as AccessibilityEvent,
    WindowEvent as AccessibilityWindowEvent,
};
use superi_gpu::device::{
    AdapterSelection, DeviceRequest, GpuDevice, GpuInstance, InstanceOptions,
};
use superi_gpu::submission::GpuSubmissionQueue;
use superi_gpu::surface::{NativeViewportSurface, ViewportExtent};
use superi_session::SessionOwners;
use superi_ui::fixture::{FoundationFixture, FoundationState};
use superi_ui::input::{InputEvent, InteractionController, Key};
use superi_ui::renderer::encode_scene_to_view;
use superi_ui::semantics::accesskit_id;
use winit::application::ApplicationHandler;
use winit::dpi::{LogicalSize, PhysicalPosition};
use winit::event::{ElementState, MouseButton, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop, EventLoopProxy};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

/// Explicit launch policy for the native host.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DesktopLaunchOptions {
    smoke: bool,
    logical_width: u32,
    logical_height: u32,
    session_root: Option<PathBuf>,
    legacy_root: Option<PathBuf>,
}

impl DesktopLaunchOptions {
    /// Parses the small native-host command line.
    pub fn parse(arguments: impl IntoIterator<Item = String>) -> Result<Self, String> {
        let arguments = arguments.into_iter().collect::<Vec<_>>();
        let mut options = Self {
            smoke: false,
            logical_width: 1440,
            logical_height: 900,
            session_root: None,
            legacy_root: None,
        };
        let mut index = 0;
        while index < arguments.len() {
            match arguments[index].as_str() {
                "--smoke" => {
                    options.smoke = true;
                    index += 1;
                }
                "--width" | "--height" => {
                    let name = arguments[index].clone();
                    let value = arguments
                        .get(index + 1)
                        .ok_or_else(|| format!("option `{name}` requires a value"))?
                        .parse::<u32>()
                        .map_err(|_| format!("option `{name}` requires an unsigned integer"))?;
                    if name == "--width" {
                        options.logical_width = value;
                    } else {
                        options.logical_height = value;
                    }
                    index += 2;
                }
                "--session-root" | "--legacy-root" => {
                    let name = arguments[index].clone();
                    let value = arguments
                        .get(index + 1)
                        .ok_or_else(|| format!("option `{name}` requires a value"))?;
                    if value.trim().is_empty() {
                        return Err(format!("option `{name}` requires a nonempty path"));
                    }
                    if name == "--session-root" {
                        options.session_root = Some(PathBuf::from(value));
                    } else {
                        options.legacy_root = Some(PathBuf::from(value));
                    }
                    index += 2;
                }
                other => return Err(format!("unknown native host option `{other}`")),
            }
        }
        if options.logical_width < 960 || options.logical_height < 640 {
            return Err("native host dimensions must be at least 960 by 640".to_owned());
        }
        Ok(options)
    }

    /// Returns whether to exit after one presented product frame.
    #[must_use]
    pub const fn smoke(&self) -> bool {
        self.smoke
    }

    /// Returns requested logical client size.
    #[must_use]
    pub const fn logical_size(&self) -> (u32, u32) {
        (self.logical_width, self.logical_height)
    }

    /// Returns an explicit portable session root when one was supplied.
    #[must_use]
    pub fn session_root(&self) -> Option<&Path> {
        self.session_root.as_deref()
    }

    /// Returns an explicit legacy source root when migration was requested.
    #[must_use]
    pub fn legacy_root(&self) -> Option<&Path> {
        self.legacy_root.as_deref()
    }
}

/// Portable services retained for the full native process lifetime.
pub struct DesktopSession {
    owners: SessionOwners,
    _temporary_root: Option<tempfile::TempDir>,
}

impl DesktopSession {
    /// Initializes portable session services without writing smoke state into user storage.
    pub fn initialize(options: &DesktopLaunchOptions) -> Result<Self, String> {
        let temporary_root = if options.smoke() && options.session_root().is_none() {
            Some(tempfile::tempdir().map_err(|error| format!("create smoke session: {error}"))?)
        } else {
            None
        };
        let session_root = options
            .session_root()
            .map(Path::to_path_buf)
            .or_else(|| {
                temporary_root
                    .as_ref()
                    .map(|root| root.path().join("session"))
            })
            .map_or_else(default_session_root, Ok)?;
        let (owners, _) = SessionOwners::initialize(options.legacy_root(), session_root)
            .map_err(|error| format!("initialize portable desktop session: {error}"))?;
        Ok(Self {
            owners,
            _temporary_root: temporary_root,
        })
    }

    /// Returns the portable session owner used by the native host.
    #[must_use]
    pub const fn owners(&self) -> &SessionOwners {
        &self.owners
    }
}

/// Runs the native event loop until the user closes it or smoke mode presents once.
pub fn run(options: DesktopLaunchOptions) -> Result<(), String> {
    let session = DesktopSession::initialize(&options)?;
    let event_loop = EventLoop::<AccessibilityEvent>::with_user_event()
        .build()
        .map_err(|error| error.to_string())?;
    event_loop.set_control_flow(ControlFlow::Wait);
    let mut application = NativeApplication::new(options, session, event_loop.create_proxy());
    event_loop
        .run_app(&mut application)
        .map_err(|error| error.to_string())?;
    if let Some(error) = application.fatal_error {
        Err(error)
    } else {
        Ok(())
    }
}

struct NativeApplication {
    options: DesktopLaunchOptions,
    _session: DesktopSession,
    accessibility_proxy: EventLoopProxy<AccessibilityEvent>,
    accessibility: Option<AccessibilityAdapter>,
    window: Option<Arc<Window>>,
    instance: Option<GpuInstance>,
    viewport: Option<NativeViewportSurface>,
    device: Option<GpuDevice>,
    controller: InteractionController,
    cursor: PhysicalPosition<f64>,
    presented_frames: u64,
    shift_active: bool,
    recovery_attempted: bool,
    fatal_error: Option<String>,
}

impl NativeApplication {
    fn new(
        options: DesktopLaunchOptions,
        session: DesktopSession,
        accessibility_proxy: EventLoopProxy<AccessibilityEvent>,
    ) -> Self {
        Self {
            options,
            _session: session,
            accessibility_proxy,
            accessibility: None,
            window: None,
            instance: None,
            viewport: None,
            device: None,
            controller: InteractionController::new(FoundationState::default()),
            cursor: PhysicalPosition::new(0.0, 0.0),
            presented_frames: 0,
            shift_active: false,
            recovery_attempted: false,
            fatal_error: None,
        }
    }

    fn initialize(&mut self, event_loop: &ActiveEventLoop) -> Result<(), String> {
        let (width, height) = self.options.logical_size();
        let attributes = Window::default_attributes()
            .with_title("Superi")
            .with_inner_size(LogicalSize::new(width, height))
            .with_min_inner_size(LogicalSize::new(960, 640))
            .with_visible(false);
        let window = Arc::new(
            event_loop
                .create_window(attributes)
                .map_err(|error| format!("create native window: {error}"))?,
        );
        let accessibility = AccessibilityAdapter::with_event_loop_proxy(
            event_loop,
            &window,
            self.accessibility_proxy.clone(),
        );
        let instance =
            GpuInstance::new(InstanceOptions::default()).map_err(|error| error.to_string())?;
        let mut viewport = NativeViewportSurface::create(&instance, Arc::clone(&window))
            .map_err(|error| error.to_string())?;
        let selected = viewport
            .compatible_adapters(&instance)
            .map_err(|error| error.to_string())?
            .select(&AdapterSelection::default())
            .map_err(|error| error.to_string())?;
        let device = pollster::block_on(
            selected.create_device(&DeviceRequest::default().with_label("superi native desktop")),
        )
        .map_err(|error| error.to_string())?;
        configure(&mut viewport, &device, &window)?;
        self.window = Some(window);
        self.accessibility = Some(accessibility);
        self.instance = Some(instance);
        self.viewport = Some(viewport);
        self.device = Some(device);
        if let Some(window) = &self.window {
            window.set_visible(true);
        }
        self.request_redraw();
        Ok(())
    }

    fn scene(&self) -> Result<superi_ui::scene::Scene, String> {
        let window = self
            .window
            .as_ref()
            .ok_or_else(|| "native window is unavailable".to_owned())?;
        let size = window.inner_size();
        let scale = window.scale_factor() as f32;
        let logical_width = ((size.width as f64 / f64::from(scale)).round() as u32).max(960);
        let logical_height = ((size.height as f64 / f64::from(scale)).round() as u32).max(640);
        FoundationFixture::new(logical_width, logical_height, scale)
            .map_err(|error| error.to_string())?
            .scene(self.controller.state())
            .map_err(|error| error.to_string())
    }

    fn present(&mut self) -> Result<(), String> {
        let scene = self.scene()?;
        let device = self
            .device
            .as_ref()
            .ok_or_else(|| "native GPU device is unavailable".to_owned())?;
        let viewport = self
            .viewport
            .as_mut()
            .ok_or_else(|| "native viewport is unavailable".to_owned())?;
        let format = viewport
            .configuration()
            .ok_or_else(|| "native viewport is not configured".to_owned())?
            .format;
        let submissions = GpuSubmissionQueue::new(device).map_err(|error| error.to_string())?;
        let frame = viewport
            .acquire_frame(device)
            .map_err(|error| error.to_string())?;
        let view = frame
            .texture()
            .create_view(&superi_gpu::wgpu::TextureViewDescriptor::default());
        let encoded = encode_scene_to_view(device, &scene, &view, format)
            .map_err(|error| error.to_string())?;
        let (command_buffer, retained) = encoded.into_submission(&submissions);
        let fence = frame
            .submit_and_present(&submissions, [command_buffer], retained)
            .map_err(|error| error.to_string())?;
        submissions
            .wait(&fence)
            .map_err(|error| error.to_string())?;
        self.presented_frames = self.presented_frames.saturating_add(1);
        self.recovery_attempted = false;
        Ok(())
    }

    fn recover_device(&mut self) -> Result<(), String> {
        let instance = self
            .instance
            .as_ref()
            .ok_or_else(|| "GPU instance is unavailable for recovery".to_owned())?;
        let viewport = self
            .viewport
            .as_mut()
            .ok_or_else(|| "native viewport is unavailable for recovery".to_owned())?;
        let window = self
            .window
            .as_ref()
            .ok_or_else(|| "native window is unavailable for recovery".to_owned())?;
        let selected = viewport
            .compatible_adapters(instance)
            .map_err(|error| error.to_string())?
            .select(&AdapterSelection::default())
            .map_err(|error| error.to_string())?;
        let device = pollster::block_on(
            selected
                .create_device(&DeviceRequest::default().with_label("superi recovered desktop")),
        )
        .map_err(|error| error.to_string())?;
        configure(viewport, &device, window)?;
        self.device = Some(device);
        Ok(())
    }

    fn reconfigure(&mut self) -> Result<(), String> {
        let window = self
            .window
            .as_ref()
            .ok_or_else(|| "native window is unavailable".to_owned())?;
        let device = self
            .device
            .as_ref()
            .ok_or_else(|| "native GPU device is unavailable".to_owned())?;
        let viewport = self
            .viewport
            .as_mut()
            .ok_or_else(|| "native viewport is unavailable".to_owned())?;
        configure(viewport, device, window)
    }

    fn dispatch(&mut self, event: InputEvent) -> Result<(), String> {
        let scene = self.scene()?;
        self.controller
            .dispatch(&scene, event)
            .map_err(|error| error.to_string())?;
        self.publish_accessibility()?;
        self.request_redraw();
        Ok(())
    }

    fn publish_accessibility(&mut self) -> Result<(), String> {
        let update = self
            .scene()?
            .semantics()
            .accesskit_update()
            .map_err(|error| error.to_string())?;
        if let Some(adapter) = &mut self.accessibility {
            adapter.update_if_active(|| update);
        }
        Ok(())
    }

    fn retained_id(
        &self,
        target: AccessNodeId,
    ) -> Result<Option<superi_ui::scene::NodeId>, String> {
        let semantics = self.scene()?.semantics();
        Ok(semantics
            .nodes()
            .iter()
            .find(|node| accesskit_id(node.id()) == target)
            .map(|node| node.id().clone()))
    }

    fn request_redraw(&self) {
        if let Some(window) = &self.window {
            window.request_redraw();
        }
    }

    fn fail(&mut self, event_loop: &ActiveEventLoop, error: String) {
        self.fatal_error = Some(error);
        event_loop.exit();
    }
}

fn default_session_root() -> Result<PathBuf, String> {
    #[cfg(target_os = "macos")]
    {
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| {
                home.join("Library")
                    .join("Application Support")
                    .join("Superi")
                    .join("session")
            })
            .ok_or_else(|| "HOME is unavailable for the native session root".to_owned());
    }
    #[cfg(target_os = "windows")]
    {
        return std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .map(|root| root.join("Superi").join("session"))
            .ok_or_else(|| "APPDATA is unavailable for the native session root".to_owned());
    }
    #[cfg(all(unix, not(target_os = "macos")))]
    {
        if let Some(root) = std::env::var_os("XDG_STATE_HOME") {
            return Ok(PathBuf::from(root).join("superi").join("session"));
        }
        return std::env::var_os("HOME")
            .map(PathBuf::from)
            .map(|home| {
                home.join(".local")
                    .join("state")
                    .join("superi")
                    .join("session")
            })
            .ok_or_else(|| "HOME is unavailable for the native session root".to_owned());
    }
    #[allow(unreachable_code)]
    Err("this platform has no declared native session root".to_owned())
}

impl ApplicationHandler<AccessibilityEvent> for NativeApplication {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.window.is_none() {
            if let Err(error) = self.initialize(event_loop) {
                self.fail(event_loop, error);
            }
        }
    }

    fn window_event(
        &mut self,
        event_loop: &ActiveEventLoop,
        _window_id: WindowId,
        event: WindowEvent,
    ) {
        if let (Some(adapter), Some(window)) = (&mut self.accessibility, &self.window) {
            adapter.process_event(window, &event);
        }
        let result = match event {
            WindowEvent::CloseRequested => {
                event_loop.exit();
                Ok(())
            }
            WindowEvent::Resized(_) | WindowEvent::ScaleFactorChanged { .. } => {
                self.reconfigure().map(|()| self.request_redraw())
            }
            WindowEvent::CursorMoved { position, .. } => {
                self.cursor = position;
                Ok(())
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                self.shift_active = modifiers.state().shift_key();
                Ok(())
            }
            WindowEvent::MouseInput {
                state: ElementState::Released,
                button: MouseButton::Left,
                ..
            } => {
                let scale = self
                    .window
                    .as_ref()
                    .map_or(1.0, |window| window.scale_factor());
                self.dispatch(InputEvent::Pointer {
                    x: (self.cursor.x / scale) as f32,
                    y: (self.cursor.y / scale) as f32,
                })
            }
            WindowEvent::KeyboardInput { event, .. } if event.state == ElementState::Pressed => {
                let command = match event.physical_key {
                    PhysicalKey::Code(KeyCode::Tab) if self.shift_active => Some(Key::ShiftTab),
                    PhysicalKey::Code(KeyCode::Tab) => Some(Key::Tab),
                    PhysicalKey::Code(KeyCode::Enter) => Some(Key::Enter),
                    PhysicalKey::Code(KeyCode::Space) => Some(Key::Space),
                    PhysicalKey::Code(KeyCode::Escape) => Some(Key::Escape),
                    PhysicalKey::Code(KeyCode::ArrowLeft) => Some(Key::ArrowLeft),
                    PhysicalKey::Code(KeyCode::ArrowRight) => Some(Key::ArrowRight),
                    _ => None,
                };
                if let Some(command) = command {
                    self.dispatch(InputEvent::Key(command))
                } else if let Some(text) = event
                    .text
                    .filter(|text| !text.chars().any(char::is_control) && !text.is_empty())
                {
                    self.dispatch(InputEvent::Text(text.to_string()))
                } else {
                    Ok(())
                }
            }
            WindowEvent::RedrawRequested if self.options.smoke() && self.presented_frames > 0 => {
                event_loop.exit();
                Ok(())
            }
            WindowEvent::RedrawRequested => match self.present() {
                Ok(()) => {
                    if self.options.smoke() {
                        println!("presented Superi retained scene through native wgpu surface");
                        event_loop.exit();
                    }
                    Ok(())
                }
                Err(first_error) if !self.recovery_attempted => {
                    self.recovery_attempted = true;
                    self.recover_device().map(|()| {
                        eprintln!("native GPU path recovered after: {first_error}");
                        self.request_redraw();
                    })
                }
                Err(error) => Err(error),
            },
            _ => Ok(()),
        };
        if let Err(error) = result {
            self.fail(event_loop, error);
        }
    }

    fn user_event(&mut self, event_loop: &ActiveEventLoop, event: AccessibilityEvent) {
        if self
            .window
            .as_ref()
            .map_or(true, |window| window.id() != event.window_id)
        {
            return;
        }
        let result = match event.window_event {
            AccessibilityWindowEvent::InitialTreeRequested => self.publish_accessibility(),
            AccessibilityWindowEvent::ActionRequested(ActionRequest {
                action,
                target_node,
                ..
            }) => match (action, self.retained_id(target_node)) {
                (_, Err(error)) => Err(error),
                (AccessAction::Focus, Ok(Some(target))) => self.dispatch(InputEvent::Focus(target)),
                (AccessAction::Click, Ok(Some(target))) => {
                    self.dispatch(InputEvent::Activate(target))
                }
                (_, Ok(_)) => Ok(()),
            },
            AccessibilityWindowEvent::AccessibilityDeactivated => Ok(()),
        };
        if let Err(error) = result {
            self.fail(event_loop, error);
        }
    }
}

fn configure(
    viewport: &mut NativeViewportSurface,
    device: &GpuDevice,
    window: &Window,
) -> Result<(), String> {
    let size = window.inner_size();
    if size.width == 0 || size.height == 0 {
        return Ok(());
    }
    let extent = ViewportExtent::new(size.width, size.height, window.scale_factor())
        .map_err(|error| error.to_string())?;
    viewport
        .configure(device, extent)
        .map(|_| ())
        .map_err(|error| error.to_string())
}
