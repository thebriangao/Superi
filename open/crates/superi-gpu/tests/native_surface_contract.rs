use std::num::{NonZeroIsize, NonZeroU32};
use std::ptr::NonNull;
use std::sync::Arc;

use raw_window_handle::{
    AppKitDisplayHandle, AppKitWindowHandle, DisplayHandle, HandleError, HasDisplayHandle,
    HasWindowHandle, RawDisplayHandle, RawWindowHandle, WaylandDisplayHandle, WaylandWindowHandle,
    Win32WindowHandle, WinRtWindowHandle, WindowHandle, WindowsDisplayHandle, XcbDisplayHandle,
    XcbWindowHandle, XlibDisplayHandle, XlibWindowHandle,
};
use superi_core::error::{ErrorCategory, Recoverability};
use superi_gpu::device::{GpuInstance, InstanceOptions};
use superi_gpu::surface::{
    NativeViewportKind, NativeViewportSurface, ViewportExtent, ViewportHost,
};

fn opaque_pointer() -> NonNull<std::ffi::c_void> {
    NonNull::from(&()).cast()
}

struct UnavailableHost;

impl HasDisplayHandle for UnavailableHost {
    fn display_handle(&self) -> Result<DisplayHandle<'_>, HandleError> {
        Err(HandleError::Unavailable)
    }
}

impl HasWindowHandle for UnavailableHost {
    fn window_handle(&self) -> Result<WindowHandle<'_>, HandleError> {
        Err(HandleError::Unavailable)
    }
}

#[test]
fn classifies_supported_desktop_handle_pairs() {
    let cases = [
        (
            RawDisplayHandle::AppKit(AppKitDisplayHandle::new()),
            RawWindowHandle::AppKit(AppKitWindowHandle::new(opaque_pointer())),
            NativeViewportKind::MacOs,
        ),
        (
            RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
            RawWindowHandle::Win32(Win32WindowHandle::new(NonZeroIsize::new(1).unwrap())),
            NativeViewportKind::Windows,
        ),
        (
            RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
            RawWindowHandle::WinRt(WinRtWindowHandle::new(opaque_pointer())),
            NativeViewportKind::Windows,
        ),
        (
            RawDisplayHandle::Xlib(XlibDisplayHandle::new(Some(opaque_pointer()), 0)),
            RawWindowHandle::Xlib(XlibWindowHandle::new(1)),
            NativeViewportKind::LinuxXlib,
        ),
        (
            RawDisplayHandle::Xcb(XcbDisplayHandle::new(Some(opaque_pointer()), 0)),
            RawWindowHandle::Xcb(XcbWindowHandle::new(NonZeroU32::new(1).unwrap())),
            NativeViewportKind::LinuxXcb,
        ),
        (
            RawDisplayHandle::Wayland(WaylandDisplayHandle::new(opaque_pointer())),
            RawWindowHandle::Wayland(WaylandWindowHandle::new(opaque_pointer())),
            NativeViewportKind::LinuxWayland,
        ),
    ];

    for (display, window, expected) in cases {
        assert_eq!(
            NativeViewportKind::from_raw_handles(display, window).unwrap(),
            expected
        );
    }
}

#[test]
fn rejects_mismatched_or_non_desktop_handle_pairs() {
    let error = NativeViewportKind::from_raw_handles(
        RawDisplayHandle::Windows(WindowsDisplayHandle::new()),
        RawWindowHandle::Xlib(XlibWindowHandle::new(1)),
    )
    .unwrap_err();

    assert_eq!(error.category(), ErrorCategory::Unsupported);
    assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    assert_eq!(error.contexts()[0].field("display_handle"), Some("windows"));
    assert_eq!(error.contexts()[0].field("window_handle"), Some("xlib"));
}

#[test]
fn validates_physical_extent_and_scale() {
    let extent = ViewportExtent::new(3840, 2160, 2.0).unwrap();
    assert_eq!(extent.width(), 3840);
    assert_eq!(extent.height(), 2160);
    assert_eq!(extent.scale_factor(), 2.0);

    for (width, height, scale) in [
        (0, 2160, 2.0),
        (3840, 0, 2.0),
        (3840, 2160, 0.0),
        (3840, 2160, f64::NAN),
        (3840, 2160, f64::INFINITY),
    ] {
        let error = ViewportExtent::new(width, height, scale).unwrap_err();
        assert_eq!(error.category(), ErrorCategory::InvalidInput);
        assert_eq!(error.recoverability(), Recoverability::UserCorrectable);
    }
}

#[test]
fn reports_current_native_target_without_runtime_guessing() {
    #[cfg(target_os = "macos")]
    assert!(NativeViewportKind::MacOs.is_supported_on_current_target());
    #[cfg(target_os = "windows")]
    assert!(NativeViewportKind::Windows.is_supported_on_current_target());
    #[cfg(target_os = "linux")]
    {
        assert!(NativeViewportKind::LinuxXlib.is_supported_on_current_target());
        assert!(NativeViewportKind::LinuxXcb.is_supported_on_current_target());
        assert!(NativeViewportKind::LinuxWayland.is_supported_on_current_target());
    }

    #[cfg(not(target_os = "windows"))]
    assert!(!NativeViewportKind::Windows.is_supported_on_current_target());
}

#[test]
fn unavailable_shell_handles_remain_retryable() {
    fn assert_viewport_host<T: ViewportHost>() {}
    assert_viewport_host::<UnavailableHost>();

    let instance = GpuInstance::new(InstanceOptions::default()).unwrap();
    let error = NativeViewportSurface::create(&instance, Arc::new(UnavailableHost))
        .err()
        .expect("unavailable display handle must reject surface creation");

    assert_eq!(error.category(), ErrorCategory::Unavailable);
    assert_eq!(error.recoverability(), Recoverability::Retryable);
    assert_eq!(error.contexts()[0].operation(), "get_display_handle");
}
