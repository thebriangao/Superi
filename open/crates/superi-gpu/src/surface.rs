//! Native wgpu viewport surface ownership, configuration, and presentation.
//!
//! The application shell owns the native child view and supplies a handle provider through
//! [`ViewportHost`]. Surface creation passes an owned `Arc` provider to wgpu, which retains that
//! provider for the lifetime of the surface. This keeps the native view alive without a Superi
//! unsafe block and keeps frame textures on the GPU through presentation.

use std::marker::PhantomData;
use std::sync::Arc;

use raw_window_handle::{HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::device::{AdapterCatalog, GpuDevice, GpuInstance};

/// A shell-owned native child view that can create a wgpu presentation surface.
///
/// The host must keep the exact window and display handles stable for its lifetime. On macOS,
/// callers must create the surface on the operating-system main thread, as required by wgpu.
pub trait ViewportHost: HasDisplayHandle + HasWindowHandle + Send + Sync + 'static {}

impl<T> ViewportHost for T where T: HasDisplayHandle + HasWindowHandle + Send + Sync + 'static {}

/// One supported desktop native viewport handle family.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum NativeViewportKind {
    /// An AppKit `NSView` on macOS.
    MacOs,
    /// A Win32 or WinRT view on Windows.
    Windows,
    /// An Xlib window on Linux.
    LinuxXlib,
    /// An XCB window on Linux.
    LinuxXcb,
    /// A Wayland surface on Linux.
    LinuxWayland,
}

impl NativeViewportKind {
    /// Identifies a supported native viewport from one display and window handle pair.
    pub fn from_raw_handles(display: RawDisplayHandle, window: RawWindowHandle) -> Result<Self> {
        let kind = match (display, window) {
            (RawDisplayHandle::AppKit(_), RawWindowHandle::AppKit(_)) => Self::MacOs,
            (
                RawDisplayHandle::Windows(_),
                RawWindowHandle::Win32(_) | RawWindowHandle::WinRt(_),
            ) => Self::Windows,
            (RawDisplayHandle::Xlib(_), RawWindowHandle::Xlib(_)) => Self::LinuxXlib,
            (RawDisplayHandle::Xcb(_), RawWindowHandle::Xcb(_)) => Self::LinuxXcb,
            (RawDisplayHandle::Wayland(_), RawWindowHandle::Wayland(_)) => Self::LinuxWayland,
            _ => {
                return Err(Error::new(
                    ErrorCategory::Unsupported,
                    Recoverability::UserCorrectable,
                    "native viewport handles are not a supported matching desktop pair",
                )
                .with_context(
                    ErrorContext::new("superi-gpu.surface", "classify_native_viewport")
                        .with_field("display_handle", display_handle_name(display))
                        .with_field("window_handle", window_handle_name(window)),
                ));
            }
        };

        Ok(kind)
    }

    /// Returns the stable diagnostic code for this native handle family.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::MacOs => "macos-appkit",
            Self::Windows => "windows",
            Self::LinuxXlib => "linux-xlib",
            Self::LinuxXcb => "linux-xcb",
            Self::LinuxWayland => "linux-wayland",
        }
    }

    /// Reports whether this handle family belongs to the current compilation target.
    #[must_use]
    pub const fn is_supported_on_current_target(self) -> bool {
        match self {
            Self::MacOs => cfg!(target_os = "macos"),
            Self::Windows => cfg!(target_os = "windows"),
            Self::LinuxXlib | Self::LinuxXcb | Self::LinuxWayland => {
                cfg!(target_os = "linux")
            }
        }
    }
}

/// A validated physical viewport extent and its logical-to-physical scale.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ViewportExtent {
    width: u32,
    height: u32,
    scale_factor: f64,
}

impl ViewportExtent {
    /// Creates a non-empty physical extent with a finite positive scale factor.
    pub fn new(width: u32, height: u32, scale_factor: f64) -> Result<Self> {
        if width == 0 || height == 0 || !scale_factor.is_finite() || scale_factor <= 0.0 {
            return Err(Error::new(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "viewport extent requires non-zero physical dimensions and a finite positive scale",
            )
            .with_context(
                ErrorContext::new("superi-gpu.surface", "validate_viewport_extent")
                    .with_field("width", width.to_string())
                    .with_field("height", height.to_string())
                    .with_field("scale_factor", scale_factor.to_string()),
            ));
        }

        Ok(Self {
            width,
            height,
            scale_factor,
        })
    }

    /// Returns the physical pixel width.
    #[must_use]
    pub const fn width(self) -> u32 {
        self.width
    }

    /// Returns the physical pixel height.
    #[must_use]
    pub const fn height(self) -> u32 {
        self.height
    }

    /// Returns the logical-to-physical scale factor supplied by the shell.
    #[must_use]
    pub const fn scale_factor(self) -> f64 {
        self.scale_factor
    }
}

/// An owned native presentation surface.
///
/// The surface owns an `Arc` clone of its [`ViewportHost`] internally through wgpu. Configuration
/// requires an adapter and device from the device-selection checkpoint. Acquired frames borrow this
/// value mutably, preventing reconfiguration or a second acquisition until the frame is presented
/// or discarded.
pub struct NativeViewportSurface {
    surface: wgpu::Surface<'static>,
    kind: NativeViewportKind,
    instance_identity: Arc<()>,
    configured_device_identity: Option<Arc<()>>,
    configuration: Option<wgpu::SurfaceConfiguration>,
    extent: Option<ViewportExtent>,
    generation: u64,
    next_frame_sequence: u64,
}

impl NativeViewportSurface {
    /// Creates a native surface while transferring an owned host reference into wgpu.
    pub fn create<H>(instance: &GpuInstance, host: Arc<H>) -> Result<Self>
    where
        H: ViewportHost,
    {
        let display = host.display_handle().map_err(|source| {
            handle_error(
                "get_display_handle",
                "native display handle is unavailable",
                source,
            )
        })?;
        let window = host.window_handle().map_err(|source| {
            handle_error(
                "get_window_handle",
                "native window handle is unavailable",
                source,
            )
        })?;
        let kind = NativeViewportKind::from_raw_handles(display.as_raw(), window.as_raw())?;

        if !kind.is_supported_on_current_target() {
            return Err(Error::new(
                ErrorCategory::Unsupported,
                Recoverability::UserCorrectable,
                "native viewport handle does not belong to the current desktop target",
            )
            .with_context(
                ErrorContext::new("superi-gpu.surface", "create_native_surface")
                    .with_field("viewport_kind", kind.code())
                    .with_field("target_os", std::env::consts::OS),
            ));
        }

        let surface = instance
            .wgpu_instance()
            .create_surface(host)
            .map_err(|source| {
                Error::with_source(
                    ErrorCategory::Unavailable,
                    Recoverability::Retryable,
                    "wgpu could not create the native viewport surface",
                    source,
                )
                .with_context(
                    ErrorContext::new("superi-gpu.surface", "create_native_surface")
                        .with_field("viewport_kind", kind.code()),
                )
            })?;

        Ok(Self {
            surface,
            kind,
            instance_identity: Arc::clone(instance.identity()),
            configured_device_identity: None,
            configuration: None,
            extent: None,
            generation: 0,
            next_frame_sequence: 0,
        })
    }

    /// Enumerates adapters from the owning instance and keeps only presentation-compatible ones.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn compatible_adapters(&self, instance: &GpuInstance) -> Result<AdapterCatalog> {
        if !Arc::ptr_eq(&self.instance_identity, instance.identity()) {
            return Err(Error::new(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "native viewport surface and GPU instance do not share ownership",
            )
            .with_context(
                ErrorContext::new("superi-gpu.surface", "enumerate_surface_adapters")
                    .with_field("viewport_kind", self.kind.code()),
            ));
        }

        Ok(instance
            .enumerate_adapters()
            .retain_surface_compatible(&self.surface))
    }

    /// Returns the native handle family used by this surface.
    #[must_use]
    pub const fn kind(&self) -> NativeViewportKind {
        self.kind
    }

    /// Returns the latest successful surface configuration.
    #[must_use]
    pub const fn configuration(&self) -> Option<&wgpu::SurfaceConfiguration> {
        self.configuration.as_ref()
    }

    /// Returns the extent of the latest successful configuration.
    #[must_use]
    pub const fn extent(&self) -> Option<ViewportExtent> {
        self.extent
    }

    /// Returns the monotonic configuration generation, starting at zero before configuration.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Configures the surface for visible low-latency presentation.
    ///
    /// Callers must drop or present any prior [`ViewportFrame`] first. The selected adapter must be
    /// compatible with this surface. sRGB output is preferred when the platform exposes it, FIFO
    /// presentation is required for predictable cross-platform ordering, and opaque composition is
    /// preferred for the dedicated child view.
    pub fn configure(
        &mut self,
        device: &GpuDevice,
        extent: ViewportExtent,
    ) -> Result<&wgpu::SurfaceConfiguration> {
        let capabilities = self.surface.get_capabilities(device.wgpu_adapter());
        let format = capabilities
            .formats
            .iter()
            .copied()
            .find(wgpu::TextureFormat::is_srgb)
            .or_else(|| capabilities.formats.first().copied())
            .ok_or_else(|| unsupported_surface(self.kind, "surface exposes no texture formats"))?;

        if !capabilities
            .present_modes
            .contains(&wgpu::PresentMode::Fifo)
        {
            return Err(unsupported_surface(
                self.kind,
                "surface does not expose the required FIFO present mode",
            ));
        }
        if !capabilities
            .usages
            .contains(wgpu::TextureUsages::RENDER_ATTACHMENT)
        {
            return Err(unsupported_surface(
                self.kind,
                "surface does not support render-attachment presentation",
            ));
        }

        let alpha_mode = if capabilities
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::Opaque)
        {
            wgpu::CompositeAlphaMode::Opaque
        } else if capabilities
            .alpha_modes
            .contains(&wgpu::CompositeAlphaMode::Auto)
        {
            wgpu::CompositeAlphaMode::Auto
        } else {
            capabilities.alpha_modes.first().copied().ok_or_else(|| {
                unsupported_surface(self.kind, "surface exposes no composite alpha modes")
            })?
        };

        let generation = self.generation.checked_add(1).ok_or_else(|| {
            Error::new(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "native viewport configuration generation is exhausted",
            )
            .with_context(
                ErrorContext::new("superi-gpu.surface", "configure_native_surface")
                    .with_field("viewport_kind", self.kind.code()),
            )
        })?;
        let configuration = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: extent.width(),
            height: extent.height(),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode,
            view_formats: Vec::new(),
        };

        self.surface.configure(device.wgpu_device(), &configuration);
        self.configured_device_identity = Some(Arc::clone(device.identity()));
        self.extent = Some(extent);
        self.generation = generation;
        Ok(self.configuration.insert(configuration))
    }

    /// Acquires one GPU-resident presentation texture.
    ///
    /// The returned frame mutably borrows the surface. Submit all rendering work that references
    /// its texture before calling [`ViewportFrame::submit_and_present`]. Dropping it without
    /// presentation explicitly discards the acquired image through wgpu.
    pub fn acquire_frame<'surface, 'device>(
        &'surface mut self,
        device: &'device GpuDevice,
    ) -> Result<ViewportFrame<'surface, 'device>> {
        let Some(configured_device_identity) = self.configured_device_identity.as_ref() else {
            return Err(Error::new(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "native viewport surface must be configured before acquiring a frame",
            )
            .with_context(
                ErrorContext::new("superi-gpu.surface", "acquire_viewport_frame")
                    .with_field("viewport_kind", self.kind.code()),
            ));
        };
        if !Arc::ptr_eq(configured_device_identity, device.identity()) {
            return Err(Error::new(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "native viewport frame must use the device that configured its surface",
            )
            .with_context(
                ErrorContext::new("superi-gpu.surface", "acquire_viewport_frame")
                    .with_field("viewport_kind", self.kind.code())
                    .with_field("generation", self.generation.to_string()),
            ));
        }

        let surface_texture = self
            .surface
            .get_current_texture()
            .map_err(|source| surface_acquisition_error(self.kind, self.generation, source))?;
        let sequence = self.next_frame_sequence.checked_add(1).ok_or_else(|| {
            Error::new(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "native viewport frame sequence is exhausted",
            )
            .with_context(
                ErrorContext::new("superi-gpu.surface", "acquire_viewport_frame")
                    .with_field("viewport_kind", self.kind.code())
                    .with_field("generation", self.generation.to_string()),
            )
        })?;
        self.next_frame_sequence = sequence;
        let suboptimal = surface_texture.suboptimal;

        Ok(ViewportFrame {
            surface_texture,
            generation: self.generation,
            sequence,
            suboptimal,
            device,
            surface_borrow: PhantomData,
        })
    }
}

/// One acquired GPU-resident viewport frame.
///
/// A frame holds an exclusive logical borrow of its surface so the swapchain cannot be reconfigured
/// while its texture is alive. Presentation consumes the frame, preserving submit-then-present
/// ordering in the public API.
#[derive(Debug)]
pub struct ViewportFrame<'surface, 'device> {
    surface_texture: wgpu::SurfaceTexture,
    generation: u64,
    sequence: u64,
    suboptimal: bool,
    device: &'device GpuDevice,
    surface_borrow: PhantomData<&'surface mut NativeViewportSurface>,
}

impl ViewportFrame<'_, '_> {
    /// Returns the GPU texture that render passes target directly.
    #[must_use]
    pub const fn texture(&self) -> &wgpu::Texture {
        &self.surface_texture.texture
    }

    /// Returns the surface configuration generation that produced this frame.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns the monotonic acquisition sequence for presentation diagnostics.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Reports whether wgpu recommends reconfiguring after this usable frame.
    #[must_use]
    pub const fn is_suboptimal(&self) -> bool {
        self.suboptimal
    }

    /// Submits all rendering work through the surface device's private queue, then presents.
    pub fn submit_and_present<I>(self, command_buffers: I)
    where
        I: IntoIterator<Item = wgpu::CommandBuffer>,
    {
        self.device.submit_viewport(command_buffers);
        self.surface_texture.present();
    }
}

fn handle_error(
    operation: &'static str,
    message: &'static str,
    source: raw_window_handle::HandleError,
) -> Error {
    let (category, recoverability) = match source {
        raw_window_handle::HandleError::NotSupported => {
            (ErrorCategory::Unsupported, Recoverability::UserCorrectable)
        }
        raw_window_handle::HandleError::Unavailable => {
            (ErrorCategory::Unavailable, Recoverability::Retryable)
        }
        _ => (ErrorCategory::Unavailable, Recoverability::Retryable),
    };

    Error::with_source(category, recoverability, message, source)
        .with_context(ErrorContext::new("superi-gpu.surface", operation))
}

fn unsupported_surface(kind: NativeViewportKind, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::Unsupported,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(
        ErrorContext::new("superi-gpu.surface", "configure_native_surface")
            .with_field("viewport_kind", kind.code()),
    )
}

fn surface_acquisition_error(
    kind: NativeViewportKind,
    generation: u64,
    source: wgpu::SurfaceError,
) -> Error {
    let (category, recoverability, message) = match source {
        wgpu::SurfaceError::Timeout => (
            ErrorCategory::Timeout,
            Recoverability::Retryable,
            "native viewport frame acquisition timed out",
        ),
        wgpu::SurfaceError::Outdated => (
            ErrorCategory::Conflict,
            Recoverability::Retryable,
            "native viewport surface is outdated and must be reconfigured",
        ),
        wgpu::SurfaceError::Lost => (
            ErrorCategory::Unavailable,
            Recoverability::Retryable,
            "native viewport surface was lost and must be reconfigured",
        ),
        wgpu::SurfaceError::OutOfMemory => (
            ErrorCategory::ResourceExhausted,
            Recoverability::Terminal,
            "GPU memory was exhausted while acquiring a viewport frame",
        ),
    };

    Error::with_source(category, recoverability, message, source).with_context(
        ErrorContext::new("superi-gpu.surface", "acquire_viewport_frame")
            .with_field("viewport_kind", kind.code())
            .with_field("generation", generation.to_string()),
    )
}

fn display_handle_name(handle: RawDisplayHandle) -> &'static str {
    match handle {
        RawDisplayHandle::UiKit(_) => "uikit",
        RawDisplayHandle::AppKit(_) => "appkit",
        RawDisplayHandle::Orbital(_) => "orbital",
        RawDisplayHandle::Ohos(_) => "ohos",
        RawDisplayHandle::Xlib(_) => "xlib",
        RawDisplayHandle::Xcb(_) => "xcb",
        RawDisplayHandle::Wayland(_) => "wayland",
        RawDisplayHandle::Drm(_) => "drm",
        RawDisplayHandle::Gbm(_) => "gbm",
        RawDisplayHandle::Windows(_) => "windows",
        RawDisplayHandle::Web(_) => "web",
        RawDisplayHandle::Android(_) => "android",
        RawDisplayHandle::Haiku(_) => "haiku",
        _ => "unknown",
    }
}

fn window_handle_name(handle: RawWindowHandle) -> &'static str {
    match handle {
        RawWindowHandle::UiKit(_) => "uikit",
        RawWindowHandle::AppKit(_) => "appkit",
        RawWindowHandle::Orbital(_) => "orbital",
        RawWindowHandle::OhosNdk(_) => "ohos-ndk",
        RawWindowHandle::Xlib(_) => "xlib",
        RawWindowHandle::Xcb(_) => "xcb",
        RawWindowHandle::Wayland(_) => "wayland",
        RawWindowHandle::Drm(_) => "drm",
        RawWindowHandle::Gbm(_) => "gbm",
        RawWindowHandle::Win32(_) => "win32",
        RawWindowHandle::WinRt(_) => "winrt",
        RawWindowHandle::Web(_) => "web",
        RawWindowHandle::WebCanvas(_) => "web-canvas",
        RawWindowHandle::WebOffscreenCanvas(_) => "web-offscreen-canvas",
        RawWindowHandle::AndroidNdk(_) => "android-ndk",
        RawWindowHandle::Haiku(_) => "haiku",
        _ => "unknown",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_failures_keep_recovery_semantics() {
        let cases = [
            (
                wgpu::SurfaceError::Timeout,
                ErrorCategory::Timeout,
                Recoverability::Retryable,
            ),
            (
                wgpu::SurfaceError::Outdated,
                ErrorCategory::Conflict,
                Recoverability::Retryable,
            ),
            (
                wgpu::SurfaceError::Lost,
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
            ),
            (
                wgpu::SurfaceError::OutOfMemory,
                ErrorCategory::ResourceExhausted,
                Recoverability::Terminal,
            ),
        ];

        for (source, category, recoverability) in cases {
            let error = surface_acquisition_error(NativeViewportKind::MacOs, 7, source);
            assert_eq!(error.category(), category);
            assert_eq!(error.recoverability(), recoverability);
            assert_eq!(error.contexts()[0].field("generation"), Some("7"));
        }
    }
}
