//! Monitor-aware ownership for the native GPU viewport presentation path.
//!
//! The desktop shell publishes one immutable display-profile snapshot and the
//! monitor identity containing each viewport. This module owns the color state
//! beside the real [`NativeViewportSurface`], checks it before acquisition, and
//! checks the captured token again before presentation. A profile refresh while
//! a GPU frame is in flight therefore cannot silently present through a stale
//! output transform.

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_gpu::device::{AdapterCatalog, GpuDevice, GpuInstance};
use superi_gpu::submission::{GpuFence, GpuSubmissionQueue, GpuSubmissionResources};
use superi_gpu::surface::{
    NativeViewportKind, NativeViewportSurface, ViewportExtent, ViewportFrame,
};

use crate::icc::{
    DisplayProfileSnapshot, IccProfileId, MonitorId, MonitorPresentationBinding,
    PresentationProfileState,
};

const COMPONENT: &str = "superi-color.view";

/// Exact profile transition applied to one native viewport.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewportProfileChange {
    previous: MonitorPresentationBinding,
    current: MonitorPresentationBinding,
}

impl ViewportProfileChange {
    /// Reports whether monitor identity, catalog generation, or profile state changed.
    #[must_use]
    pub fn changed(&self) -> bool {
        self.previous != self.current
    }

    /// Returns the binding that was active before the transition.
    #[must_use]
    pub const fn previous(&self) -> &MonitorPresentationBinding {
        &self.previous
    }

    /// Returns the binding installed by the transition.
    #[must_use]
    pub const fn current(&self) -> &MonitorPresentationBinding {
        &self.current
    }
}

/// Color state owned beside one native viewport surface.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MonitorAwareViewportState {
    binding: MonitorPresentationBinding,
}

impl MonitorAwareViewportState {
    /// Binds a viewport to one active monitor in an immutable catalog snapshot.
    pub fn new(snapshot: &DisplayProfileSnapshot, monitor_id: &MonitorId) -> Result<Self> {
        Ok(Self {
            binding: snapshot.bind_for_presentation(monitor_id)?,
        })
    }

    /// Returns the exact monitor and profile state used by this viewport.
    #[must_use]
    pub const fn binding(&self) -> &MonitorPresentationBinding {
        &self.binding
    }

    /// Explicitly moves the viewport to another active monitor.
    pub fn move_to_monitor(
        &mut self,
        snapshot: &DisplayProfileSnapshot,
        monitor_id: &MonitorId,
    ) -> Result<ViewportProfileChange> {
        self.install(snapshot.bind_for_presentation(monitor_id)?)
    }

    /// Rebinds the current monitor after a native profile or display-set refresh.
    pub fn refresh_profile(
        &mut self,
        snapshot: &DisplayProfileSnapshot,
    ) -> Result<ViewportProfileChange> {
        let monitor_id = self.binding.monitor_id().clone();
        self.install(snapshot.bind_for_presentation(&monitor_id)?)
    }

    /// Captures the current binding for one acquired native presentation frame.
    pub fn frame_token(
        &self,
        snapshot: &DisplayProfileSnapshot,
        current_monitor_id: &MonitorId,
    ) -> Result<ViewportPresentationToken> {
        if self.binding.monitor_id() != current_monitor_id {
            return Err(stale_monitor_error(
                &self.binding,
                snapshot,
                current_monitor_id,
                "acquire_monitor_aware_frame",
            ));
        }
        ensure_binding_current(&self.binding, snapshot, "acquire_monitor_aware_frame")?;
        Ok(ViewportPresentationToken {
            binding: self.binding.clone(),
        })
    }

    fn install(&mut self, current: MonitorPresentationBinding) -> Result<ViewportProfileChange> {
        if current.monitor_id().as_str().is_empty() {
            return Err(Error::new(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "validated monitor binding unexpectedly has an empty identity",
            )
            .with_context(ErrorContext::new(COMPONENT, "install_viewport_profile")));
        }
        let previous = std::mem::replace(&mut self.binding, current.clone());
        Ok(ViewportProfileChange { previous, current })
    }
}

/// Immutable profile evidence captured when a native frame is acquired.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ViewportPresentationToken {
    binding: MonitorPresentationBinding,
}

impl ViewportPresentationToken {
    /// Returns the monitor identity captured for this frame.
    #[must_use]
    pub const fn monitor_id(&self) -> &MonitorId {
        self.binding.monitor_id()
    }

    /// Returns the ICC content identity captured for this frame, if profiled.
    #[must_use]
    pub fn profile_id(&self) -> Option<IccProfileId> {
        self.binding.profile_id()
    }

    /// Returns the captured explicit profiled or unprofiled state.
    #[must_use]
    pub const fn profile_state(&self) -> &PresentationProfileState {
        self.binding.state()
    }

    /// Rejects presentation if discovery changed since frame acquisition.
    pub fn ensure_current(&self, snapshot: &DisplayProfileSnapshot) -> Result<()> {
        ensure_binding_current(&self.binding, snapshot, "present_monitor_aware_frame")
    }

    /// Rejects a frame if either its monitor or profile state changed.
    pub fn ensure_current_on(
        &self,
        snapshot: &DisplayProfileSnapshot,
        current_monitor_id: &MonitorId,
    ) -> Result<()> {
        if self.binding.monitor_id() != current_monitor_id {
            return Err(stale_monitor_error(
                &self.binding,
                snapshot,
                current_monitor_id,
                "present_monitor_aware_frame",
            ));
        }
        self.ensure_current(snapshot)
    }
}

/// Real native GPU viewport paired with its required monitor profile state.
pub struct MonitorAwareViewport {
    surface: NativeViewportSurface,
    state: MonitorAwareViewportState,
}

impl MonitorAwareViewport {
    /// Takes ownership of a real native surface and binds it to one monitor.
    pub fn new(
        surface: NativeViewportSurface,
        snapshot: &DisplayProfileSnapshot,
        monitor_id: &MonitorId,
    ) -> Result<Self> {
        Ok(Self {
            surface,
            state: MonitorAwareViewportState::new(snapshot, monitor_id)?,
        })
    }

    /// Returns the native handle family of the owned GPU surface.
    #[must_use]
    pub const fn kind(&self) -> NativeViewportKind {
        self.surface.kind()
    }

    /// Returns the viewport's exact monitor/profile state.
    #[must_use]
    pub const fn state(&self) -> &MonitorAwareViewportState {
        &self.state
    }

    /// Enumerates GPU adapters compatible with the owned native surface.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn compatible_adapters(&self, instance: &GpuInstance) -> Result<AdapterCatalog> {
        self.surface.compatible_adapters(instance)
    }

    /// Configures the owned native surface for visible presentation.
    pub fn configure(
        &mut self,
        device: &GpuDevice,
        extent: ViewportExtent,
    ) -> Result<&superi_gpu::wgpu::SurfaceConfiguration> {
        self.surface.configure(device, extent)
    }

    /// Explicitly moves this viewport to another active monitor.
    pub fn move_to_monitor(
        &mut self,
        snapshot: &DisplayProfileSnapshot,
        monitor_id: &MonitorId,
    ) -> Result<ViewportProfileChange> {
        self.state.move_to_monitor(snapshot, monitor_id)
    }

    /// Rebinds this viewport after a native display-profile refresh.
    pub fn refresh_profile(
        &mut self,
        snapshot: &DisplayProfileSnapshot,
    ) -> Result<ViewportProfileChange> {
        self.state.refresh_profile(snapshot)
    }

    /// Acquires one real GPU presentation frame only when color state is current.
    pub fn acquire_frame<'surface, 'device>(
        &'surface mut self,
        snapshot: &DisplayProfileSnapshot,
        current_monitor_id: &MonitorId,
        device: &'device GpuDevice,
    ) -> Result<MonitorAwareViewportFrame<'surface, 'device>> {
        let token = self.state.frame_token(snapshot, current_monitor_id)?;
        let frame = self.surface.acquire_frame(device)?;
        Ok(MonitorAwareViewportFrame { frame, token })
    }
}

/// Acquired GPU frame carrying the profile evidence required for presentation.
pub struct MonitorAwareViewportFrame<'surface, 'device> {
    frame: ViewportFrame<'surface, 'device>,
    token: ViewportPresentationToken,
}

impl<'device> MonitorAwareViewportFrame<'_, 'device> {
    /// Returns the real GPU surface texture targeted by render passes.
    #[must_use]
    pub const fn texture(&self) -> &superi_gpu::wgpu::Texture {
        self.frame.texture()
    }

    /// Returns the profile evidence captured for this frame.
    #[must_use]
    pub const fn presentation_token(&self) -> &ViewportPresentationToken {
        &self.token
    }

    /// Returns the native surface configuration generation for this frame.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.frame.generation()
    }

    /// Returns the monotonic native frame acquisition sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.frame.sequence()
    }

    /// Reports whether the native surface recommends reconfiguration.
    #[must_use]
    pub const fn is_suboptimal(&self) -> bool {
        self.frame.is_suboptimal()
    }

    /// Submits and presents only if the captured monitor profile is still current.
    pub fn submit_and_present<I>(
        self,
        snapshot: &DisplayProfileSnapshot,
        current_monitor_id: &MonitorId,
        submissions: &GpuSubmissionQueue<'device>,
        command_buffers: I,
        retained: GpuSubmissionResources<'device>,
    ) -> Result<GpuFence>
    where
        I: IntoIterator<Item = superi_gpu::wgpu::CommandBuffer>,
    {
        self.token.ensure_current_on(snapshot, current_monitor_id)?;
        self.frame
            .submit_and_present(submissions, command_buffers, retained)
    }
}

fn stale_monitor_error(
    binding: &MonitorPresentationBinding,
    snapshot: &DisplayProfileSnapshot,
    current_monitor_id: &MonitorId,
    operation: &'static str,
) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::Retryable,
        "native viewport moved to another monitor and must be rebound before presentation",
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("bound_monitor_id", binding.monitor_id().as_str())
            .with_field("current_monitor_id", current_monitor_id.as_str())
            .with_field("snapshot_generation", snapshot.generation().to_string()),
    )
}

fn ensure_binding_current(
    binding: &MonitorPresentationBinding,
    snapshot: &DisplayProfileSnapshot,
    operation: &'static str,
) -> Result<()> {
    if binding.is_current(snapshot) {
        return Ok(());
    }
    Err(Error::new(
        ErrorCategory::Conflict,
        Recoverability::Retryable,
        "native viewport monitor profile changed and must be rebound before presentation",
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("monitor_id", binding.monitor_id().as_str())
            .with_field(
                "binding_generation",
                binding.catalog_generation().to_string(),
            )
            .with_field("snapshot_generation", snapshot.generation().to_string()),
    ))
}
