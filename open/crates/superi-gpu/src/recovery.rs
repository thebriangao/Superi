//! Device-loss recovery, deterministic resource reconstruction, and safe status notices.

use std::any::Any;
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::atomic::{AtomicU64, Ordering};

use superi_core::diagnostics::{DiagnosticEvent, DiagnosticSeverity};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::buffer::GpuBuffer;
use crate::device::{GpuDevice, GpuDeviceLoss, GpuDeviceStatus};
use crate::resource::GpuResources;
use crate::texture::GpuTexture;

const COMPONENT: &str = "superi-gpu.recovery";
static NEXT_PLAN_SCOPE: AtomicU64 = AtomicU64::new(1);

/// User-presentable phase of one device recovery attempt.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum GpuRecoveryPhase {
    /// Loss was confirmed and obsolete GPU work is paused.
    DeviceLost,
    /// Registered resources are being rebuilt on the replacement device.
    Reconstructing,
    /// Every registered resource was rebuilt successfully.
    Recovered,
    /// Device recreation or resource reconstruction failed.
    Failed,
}

impl GpuRecoveryPhase {
    /// Returns the permanent diagnostic code for this phase.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::DeviceLost => "device_lost",
            Self::Reconstructing => "reconstructing",
            Self::Recovered => "recovered",
            Self::Failed => "failed",
        }
    }
}

/// Reviewed status suitable for UI, CLI, and automation presentation.
///
/// The user message never includes driver text, resource labels, media details,
/// paths, or source chains. Internal consumers may inspect the optional
/// diagnostic event separately.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuRecoveryNotice {
    phase: GpuRecoveryPhase,
    severity: DiagnosticSeverity,
    recoverability: Option<Recoverability>,
    user_message: &'static str,
    completed: usize,
    total: usize,
    diagnostic: Option<DiagnosticEvent>,
}

impl GpuRecoveryNotice {
    fn device_lost(loss: &GpuDeviceLoss, total: usize) -> Self {
        let error = loss.error("recover_device");
        Self {
            phase: GpuRecoveryPhase::DeviceLost,
            severity: DiagnosticSeverity::Warning,
            recoverability: Some(Recoverability::Retryable),
            user_message: "The GPU connection was interrupted. Playback and rendering are paused while Superi restores GPU resources.",
            completed: 0,
            total,
            diagnostic: diagnostic("gpu.device.lost", &error),
        }
    }

    fn reconstructing(completed: usize, total: usize) -> Self {
        Self {
            phase: GpuRecoveryPhase::Reconstructing,
            severity: DiagnosticSeverity::Info,
            recoverability: None,
            user_message:
                "Superi is restoring GPU resources. Playback and rendering remain paused.",
            completed,
            total,
            diagnostic: None,
        }
    }

    fn recovered(total: usize) -> Self {
        Self {
            phase: GpuRecoveryPhase::Recovered,
            severity: DiagnosticSeverity::Info,
            recoverability: None,
            user_message: "GPU recovery completed. Playback and rendering can continue.",
            completed: total,
            total,
            diagnostic: None,
        }
    }

    fn failed(error: &Error, completed: usize, total: usize) -> Self {
        let (severity, user_message) = match error.recoverability() {
            Recoverability::Retryable => (
                DiagnosticSeverity::Error,
                "Superi could not restore GPU resources. Try recovery again.",
            ),
            Recoverability::Degraded => (
                DiagnosticSeverity::Warning,
                "GPU recovery is incomplete. Superi can continue with reduced playback or rendering capability.",
            ),
            Recoverability::UserCorrectable => (
                DiagnosticSeverity::Warning,
                "GPU recovery needs attention. Check the selected GPU or display, then try again.",
            ),
            Recoverability::Terminal => (
                DiagnosticSeverity::Error,
                "GPU recovery cannot continue safely. Save your work if possible, then restart Superi.",
            ),
            _ => (
                DiagnosticSeverity::Error,
                "GPU recovery stopped with an unknown recovery condition. Save your work if possible, then restart Superi.",
            ),
        };
        Self {
            phase: GpuRecoveryPhase::Failed,
            severity,
            recoverability: Some(error.recoverability()),
            user_message,
            completed,
            total,
            diagnostic: diagnostic("gpu.device.recovery_failed", error),
        }
    }

    /// Returns the stable recovery phase.
    #[must_use]
    pub const fn phase(&self) -> GpuRecoveryPhase {
        self.phase
    }

    /// Returns the presentation severity.
    #[must_use]
    pub const fn severity(&self) -> DiagnosticSeverity {
        self.severity
    }

    /// Returns the next-action classification for loss and failure notices.
    #[must_use]
    pub const fn recoverability(&self) -> Option<Recoverability> {
        self.recoverability
    }

    /// Returns reviewed user-safe English status text.
    #[must_use]
    pub const fn user_message(&self) -> &'static str {
        self.user_message
    }

    /// Returns the number of recipes reconstructed before this notice.
    #[must_use]
    pub const fn completed(&self) -> usize {
        self.completed
    }

    /// Returns the total number of registered reconstruction recipes.
    #[must_use]
    pub const fn total(&self) -> usize {
        self.total
    }

    /// Returns internal failure diagnostics, when this phase represents one.
    #[must_use]
    pub const fn diagnostic(&self) -> Option<&DiagnosticEvent> {
        self.diagnostic.as_ref()
    }
}

fn diagnostic(name: &'static str, error: &Error) -> Option<DiagnosticEvent> {
    DiagnosticEvent::from_error(name, COMPONENT, error).ok()
}

/// A typed key for one output in a specific reconstruction plan.
pub struct ReconstructionKey<T> {
    scope: u64,
    index: usize,
    value: PhantomData<fn() -> T>,
}

impl<T> Copy for ReconstructionKey<T> {}

impl<T> Clone for ReconstructionKey<T> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<T> fmt::Debug for ReconstructionKey<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReconstructionKey")
            .field("scope", &self.scope)
            .field("index", &self.index)
            .finish()
    }
}

type ReconstructedValue = Box<dyn Any + Send + Sync>;

/// Typed outputs prepared by a fully successful reconstruction plan.
pub struct ReconstructedResources {
    scope: u64,
    values: Vec<Option<ReconstructedValue>>,
}

impl fmt::Debug for ReconstructedResources {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("ReconstructedResources")
            .field("scope", &self.scope)
            .field("len", &self.len())
            .finish()
    }
}

impl ReconstructedResources {
    fn with_capacity(scope: u64, capacity: usize) -> Self {
        Self {
            scope,
            values: Vec::with_capacity(capacity),
        }
    }

    fn push(&mut self, value: ReconstructedValue) {
        self.values.push(Some(value));
    }

    /// Returns the number of reconstructed outputs still owned by this set.
    #[must_use]
    pub fn len(&self) -> usize {
        self.values.iter().filter(|value| value.is_some()).count()
    }

    /// Returns whether this set owns no reconstructed outputs.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Borrows one typed output from this plan.
    pub fn get<T>(&self, key: &ReconstructionKey<T>) -> Result<&T>
    where
        T: Any + Send + Sync + 'static,
    {
        self.validate_key(key, "get_reconstructed_resource")?;
        self.values
            .get(key.index)
            .and_then(Option::as_ref)
            .and_then(|value| value.downcast_ref::<T>())
            .ok_or_else(|| unavailable_key(key.index, "get_reconstructed_resource"))
    }

    /// Removes and returns one typed output from this plan.
    pub fn remove<T>(&mut self, key: &ReconstructionKey<T>) -> Result<T>
    where
        T: Any + Send + Sync + 'static,
    {
        self.validate_key(key, "remove_reconstructed_resource")?;
        let value = self
            .values
            .get_mut(key.index)
            .and_then(Option::take)
            .ok_or_else(|| unavailable_key(key.index, "remove_reconstructed_resource"))?;
        value
            .downcast::<T>()
            .map(|value| *value)
            .map_err(|_| unavailable_key(key.index, "remove_reconstructed_resource"))
    }

    fn validate_key<T>(&self, key: &ReconstructionKey<T>, operation: &'static str) -> Result<()> {
        if self.scope == key.scope {
            return Ok(());
        }
        Err(Error::new(
            ErrorCategory::Conflict,
            Recoverability::UserCorrectable,
            "reconstruction key belongs to a different recovery plan",
        )
        .with_context(
            ErrorContext::new(COMPONENT, operation)
                .with_field("expected_scope", self.scope.to_string())
                .with_field("actual_scope", key.scope.to_string()),
        ))
    }
}

fn unavailable_key(index: usize, operation: &'static str) -> Error {
    Error::new(
        ErrorCategory::Conflict,
        Recoverability::UserCorrectable,
        "reconstructed resource is not available at this dependency point",
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation).with_field("recipe_index", index.to_string()),
    )
}

/// One managed texture region initialized during reconstruction.
#[derive(Clone, Copy, Debug)]
pub struct GpuRecoveryTextureWrite {
    mip_level: u32,
    origin: wgpu::Origin3d,
    aspect: wgpu::TextureAspect,
    data_layout: wgpu::ImageDataLayout,
    size: wgpu::Extent3d,
}

impl GpuRecoveryTextureWrite {
    /// Creates a base-mip, all-aspects texture write.
    #[must_use]
    pub const fn new(data_layout: wgpu::ImageDataLayout, size: wgpu::Extent3d) -> Self {
        Self {
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
            data_layout,
            size,
        }
    }

    /// Selects the destination mip level.
    #[must_use]
    pub const fn with_mip_level(mut self, mip_level: u32) -> Self {
        self.mip_level = mip_level;
        self
    }

    /// Selects the destination origin.
    #[must_use]
    pub const fn with_origin(mut self, origin: wgpu::Origin3d) -> Self {
        self.origin = origin;
        self
    }

    /// Selects the destination texture aspect.
    #[must_use]
    pub const fn with_aspect(mut self, aspect: wgpu::TextureAspect) -> Self {
        self.aspect = aspect;
        self
    }
}

/// Validated initialization access available only while rebuilding a device.
///
/// The context dereferences to the normal managed-resource factory. Queue
/// writes remain restricted to resources owned by this replacement device and
/// are available only before the recovered result is published.
#[derive(Clone, Debug)]
pub struct GpuRecoveryContext<'device> {
    resources: GpuResources<'device>,
}

impl<'device> GpuRecoveryContext<'device> {
    fn new(device: &'device GpuDevice) -> Result<Self> {
        Ok(Self {
            resources: GpuResources::new(device)?,
        })
    }

    /// Returns the replacement device being initialized.
    #[must_use]
    pub fn device(&self) -> &GpuDevice {
        self.resources.device()
    }

    /// Returns the managed resource factory for the replacement lifetime.
    #[must_use]
    pub const fn resources(&self) -> &GpuResources<'device> {
        &self.resources
    }

    /// Initializes one managed COPY_DST buffer through the private queue.
    pub fn write_buffer(
        &self,
        buffer: &GpuBuffer,
        offset: wgpu::BufferAddress,
        data: &[u8],
    ) -> Result<()> {
        self.resources
            .ensure_owner(buffer.lease(), "reconstruct_buffer")?;
        let alignment = wgpu::COPY_BUFFER_ALIGNMENT;
        let data_len = u64::try_from(data.len()).map_err(|_| {
            invalid_initialization(
                "reconstruct_buffer",
                "buffer data length is not representable",
            )
        })?;
        let end = offset.checked_add(data_len).ok_or_else(|| {
            invalid_initialization("reconstruct_buffer", "buffer write range overflowed")
        })?;
        if !buffer.info().usage().contains(wgpu::BufferUsages::COPY_DST)
            || data.is_empty()
            || offset % alignment != 0
            || data_len % alignment != 0
            || end > buffer.info().size()
        {
            return Err(invalid_initialization(
                "reconstruct_buffer",
                "recovery buffer writes require aligned nonempty COPY_DST ranges inside the allocation",
            ));
        }
        self.device().write_buffer(buffer.raw(), offset, data)
    }

    /// Initializes one managed COPY_DST texture through the private queue.
    pub fn write_texture(
        &self,
        texture: &GpuTexture,
        data: &[u8],
        write: GpuRecoveryTextureWrite,
    ) -> Result<()> {
        self.resources
            .ensure_owner(texture.lease(), "reconstruct_texture")?;
        let info = texture.info();
        let mip_extent = mip_extent(info, write.mip_level).ok_or_else(|| {
            invalid_initialization(
                "reconstruct_texture",
                "recovery texture mip level is outside the allocation",
            )
        })?;
        let end_x = write.origin.x.checked_add(write.size.width);
        let end_y = write.origin.y.checked_add(write.size.height);
        let end_z = write.origin.z.checked_add(write.size.depth_or_array_layers);
        if !info.usage().contains(wgpu::TextureUsages::COPY_DST)
            || data.is_empty()
            || write.size.width == 0
            || write.size.height == 0
            || write.size.depth_or_array_layers == 0
            || end_x.map_or(true, |end| end > mip_extent.width)
            || end_y.map_or(true, |end| end > mip_extent.height)
            || end_z.map_or(true, |end| end > mip_extent.depth_or_array_layers)
        {
            return Err(invalid_initialization(
                "reconstruct_texture",
                "recovery texture writes require a nonempty COPY_DST region inside the selected mip",
            ));
        }
        self.device().write_texture(
            wgpu::ImageCopyTexture {
                texture: texture.raw(),
                mip_level: write.mip_level,
                origin: write.origin,
                aspect: write.aspect,
            },
            data,
            write.data_layout,
            write.size,
        )
    }
}

impl<'device> Deref for GpuRecoveryContext<'device> {
    type Target = GpuResources<'device>;

    fn deref(&self) -> &Self::Target {
        &self.resources
    }
}

fn mip_extent(info: &crate::texture::GpuTextureInfo, mip_level: u32) -> Option<wgpu::Extent3d> {
    if mip_level >= info.mip_level_count() {
        return None;
    }
    let mip_dimension = |value: u32| value.checked_shr(mip_level).unwrap_or(0).max(1);
    Some(wgpu::Extent3d {
        width: mip_dimension(info.size().width),
        height: mip_dimension(info.size().height),
        depth_or_array_layers: if info.dimension() == wgpu::TextureDimension::D3 {
            mip_dimension(info.size().depth_or_array_layers)
        } else {
            info.size().depth_or_array_layers
        },
    })
}

fn invalid_initialization(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}

type Rebuild = dyn for<'device> Fn(
        &GpuRecoveryContext<'device>,
        &ReconstructedResources,
    ) -> Result<ReconstructedValue>
    + Send
    + Sync;

struct ReconstructionRecipe {
    label: String,
    rebuild: Box<Rebuild>,
}

/// Ordered, reusable recipes for resources that must survive device replacement.
pub struct GpuRecoveryPlan {
    scope: u64,
    recipes: Vec<ReconstructionRecipe>,
}

impl fmt::Debug for GpuRecoveryPlan {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GpuRecoveryPlan")
            .field("scope", &self.scope)
            .field(
                "labels",
                &self
                    .recipes
                    .iter()
                    .map(|recipe| recipe.label.as_str())
                    .collect::<Vec<_>>(),
            )
            .finish()
    }
}

impl GpuRecoveryPlan {
    /// Creates an empty process-local reconstruction plan.
    pub fn new() -> Result<Self> {
        let scope = NEXT_PLAN_SCOPE
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                current.checked_add(1)
            })
            .map_err(|_| {
                Error::new(
                    ErrorCategory::ResourceExhausted,
                    Recoverability::Terminal,
                    "GPU recovery plan identifiers are exhausted",
                )
                .with_context(ErrorContext::new(COMPONENT, "create_recovery_plan"))
            })?;
        Ok(Self {
            scope,
            recipes: Vec::new(),
        })
    }

    /// Returns the number of registered recipes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.recipes.len()
    }

    /// Returns whether no resource reconstruction is registered.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.recipes.is_empty()
    }

    /// Registers one typed output after all existing recipes.
    ///
    /// The callback may inspect only outputs whose keys were registered earlier.
    /// It must rebuild from ordinary CPU, project, media, or cache state because
    /// resources in the lost device lifetime are not readable after loss.
    pub fn register<T, F>(
        &mut self,
        label: impl Into<String>,
        rebuild: F,
    ) -> Result<ReconstructionKey<T>>
    where
        T: Any + Send + Sync + 'static,
        F: for<'device> Fn(&GpuRecoveryContext<'device>, &ReconstructedResources) -> Result<T>
            + Send
            + Sync
            + 'static,
    {
        let label = label.into();
        if label.trim().is_empty() || self.recipes.iter().any(|recipe| recipe.label == label) {
            return Err(Error::new(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "GPU reconstruction labels must be nonempty and unique within a plan",
            )
            .with_context(ErrorContext::new(COMPONENT, "register_reconstruction")));
        }
        let index = self.recipes.len();
        self.recipes.push(ReconstructionRecipe {
            label,
            rebuild: Box::new(move |resources, reconstructed| {
                rebuild(resources, reconstructed).map(|value| Box::new(value) as ReconstructedValue)
            }),
        });
        Ok(ReconstructionKey {
            scope: self.scope,
            index,
            value: PhantomData,
        })
    }

    /// Recreates a lost device and all registered resources.
    pub async fn recover(&self, lost_device: &GpuDevice) -> Result<RecoveredGpu> {
        self.recover_with_observer(lost_device, |_| {}).await
    }

    /// Recreates a lost device while publishing reviewed progress notices.
    pub async fn recover_with_observer<F>(
        &self,
        lost_device: &GpuDevice,
        mut observer: F,
    ) -> Result<RecoveredGpu>
    where
        F: FnMut(&GpuRecoveryNotice),
    {
        let loss = match lost_device.status() {
            GpuDeviceStatus::Available { generation } => {
                return Err(Error::new(
                    ErrorCategory::Conflict,
                    Recoverability::UserCorrectable,
                    "GPU recovery requires a confirmed lost device",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "recover_device")
                        .with_field("device_generation", generation.to_string()),
                ));
            }
            GpuDeviceStatus::Lost(loss) => loss,
        };
        let total = self.recipes.len();
        observer(&GpuRecoveryNotice::device_lost(&loss, total));

        let replacement = match lost_device.recreate().await {
            Ok(device) => device,
            Err(error) => {
                observer(&GpuRecoveryNotice::failed(&error, 0, total));
                return Err(error);
            }
        };
        let context = match GpuRecoveryContext::new(&replacement) {
            Ok(context) => context,
            Err(error) => {
                observer(&GpuRecoveryNotice::failed(&error, 0, total));
                return Err(error);
            }
        };
        let mut reconstructed = ReconstructedResources::with_capacity(self.scope, total);
        observer(&GpuRecoveryNotice::reconstructing(0, total));

        for (index, recipe) in self.recipes.iter().enumerate() {
            let value = match (recipe.rebuild)(&context, &reconstructed) {
                Ok(value) => value,
                Err(error) => {
                    let error = error.with_context(
                        ErrorContext::new(COMPONENT, "reconstruct_resource")
                            .with_field("recipe_index", index.to_string())
                            .with_field("resource_label", recipe.label.clone())
                            .with_field("device_generation", replacement.generation().to_string()),
                    );
                    observer(&GpuRecoveryNotice::failed(&error, index, total));
                    return Err(error);
                }
            };
            reconstructed.push(value);
            observer(&GpuRecoveryNotice::reconstructing(index + 1, total));
        }

        let report = GpuRecoveryReport {
            previous_generation: loss.generation(),
            generation: replacement.generation(),
            resource_labels: self
                .recipes
                .iter()
                .map(|recipe| recipe.label.clone())
                .collect(),
        };
        observer(&GpuRecoveryNotice::recovered(total));
        Ok(RecoveredGpu {
            device: replacement,
            resources: reconstructed,
            report,
        })
    }
}

/// Immutable evidence from one successful recovery attempt.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuRecoveryReport {
    previous_generation: u64,
    generation: u64,
    resource_labels: Vec<String>,
}

impl GpuRecoveryReport {
    /// Returns the generation that was lost.
    #[must_use]
    pub const fn previous_generation(&self) -> u64 {
        self.previous_generation
    }

    /// Returns the replacement generation.
    #[must_use]
    pub const fn generation(&self) -> u64 {
        self.generation
    }

    /// Returns the number of successfully reconstructed resources.
    #[must_use]
    pub fn reconstructed_resources(&self) -> usize {
        self.resource_labels.len()
    }

    /// Returns reconstruction labels in exact dependency order.
    #[must_use]
    pub fn resource_labels(&self) -> &[String] {
        &self.resource_labels
    }
}

/// Replacement device and resource outputs published only after full success.
pub struct RecoveredGpu {
    device: GpuDevice,
    resources: ReconstructedResources,
    report: GpuRecoveryReport,
}

impl fmt::Debug for RecoveredGpu {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("RecoveredGpu")
            .field("device", &self.device)
            .field("resources", &self.resources)
            .field("report", &self.report)
            .finish()
    }
}

impl RecoveredGpu {
    /// Returns the replacement device.
    #[must_use]
    pub const fn device(&self) -> &GpuDevice {
        &self.device
    }

    /// Returns all reconstructed typed outputs.
    #[must_use]
    pub const fn resources(&self) -> &ReconstructedResources {
        &self.resources
    }

    /// Returns immutable recovery evidence.
    #[must_use]
    pub const fn report(&self) -> &GpuRecoveryReport {
        &self.report
    }

    /// Splits the successful result into owned parts.
    #[must_use]
    pub fn into_parts(self) -> (GpuDevice, ReconstructedResources, GpuRecoveryReport) {
        (self.device, self.resources, self.report)
    }
}

#[cfg(test)]
mod notice_contract {
    use super::*;

    #[test]
    fn failure_notices_distinguish_every_recovery_class_without_private_detail() {
        let cases = [
            (
                Recoverability::Retryable,
                DiagnosticSeverity::Error,
                "Try recovery again",
            ),
            (
                Recoverability::Degraded,
                DiagnosticSeverity::Warning,
                "reduced playback or rendering capability",
            ),
            (
                Recoverability::UserCorrectable,
                DiagnosticSeverity::Warning,
                "Check the selected GPU or display",
            ),
            (
                Recoverability::Terminal,
                DiagnosticSeverity::Error,
                "restart Superi",
            ),
        ];

        for (recoverability, severity, guidance) in cases {
            let error = Error::new(
                ErrorCategory::Internal,
                recoverability,
                "private driver and media detail",
            );
            let notice = GpuRecoveryNotice::failed(&error, 1, 2);
            assert_eq!(notice.phase(), GpuRecoveryPhase::Failed);
            assert_eq!(notice.severity(), severity);
            assert_eq!(notice.recoverability(), Some(recoverability));
            assert!(notice.user_message().contains(guidance));
            assert!(!notice
                .user_message()
                .contains("private driver and media detail"));
            assert!(notice.diagnostic().is_some());
        }
    }
}
