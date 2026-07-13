//! Privacy-safe GPU diagnostics and managed pass timing.
//!
//! Snapshots expose only adapter classes and aggregate resource, submission,
//! and managed-memory counters. Timed pass reports contain pass order, pass
//! kind, and elapsed nanoseconds. Neither boundary retains caller labels,
//! paths, resource identifiers, image dimensions, shader text, or media bytes.

use std::fmt;
use std::sync::{mpsc, Arc, Mutex, MutexGuard};
use std::time::Duration;

use superi_core::diagnostics::{DiagnosticEvent, DiagnosticSeverity, TraceField};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::buffer::GpuBuffer;
use crate::pass::GpuPassKind;
use crate::pool::{GpuMemoryPool, MemoryClass, MemoryPoolStats};
use crate::resource::{GpuResourceKind, GpuResourceStats, GpuResources};
use crate::submission::{GpuFence, GpuSubmissionProgress, GpuSubmissionQueue};

const COMPONENT: &str = "superi-gpu.diagnostics";
const WAIT_INTERVAL: Duration = Duration::from_millis(1);
const QUERIES_PER_PASS: u32 = 2;

/// A bounded request for beginning and end timestamps on managed GPU passes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuTimingConfig {
    max_passes: u32,
}

impl GpuTimingConfig {
    /// Creates a nonzero pass limit that fits one wgpu query set.
    pub fn new(max_passes: u32) -> Result<Self> {
        if max_passes == 0 {
            return Err(invalid(
                "create_timing_config",
                "GPU timing pass capacity must be greater than zero",
            ));
        }
        if max_passes > wgpu::QUERY_SET_MAX_QUERIES / QUERIES_PER_PASS {
            return Err(exhausted(
                "create_timing_config",
                "GPU timing pass capacity exceeds the wgpu query-set limit",
            ));
        }
        Ok(Self { max_passes })
    }

    /// Returns the maximum number of passes this timing batch accepts.
    #[must_use]
    pub const fn max_passes(self) -> u32 {
        self.max_passes
    }

    const fn query_count(self) -> u32 {
        self.max_passes * QUERIES_PER_PASS
    }
}

/// An aggregate snapshot that cannot contain user media or media-derived text.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuDiagnosticSnapshot {
    backend: wgpu::Backend,
    device_type: wgpu::DeviceType,
    timestamp_queries_enabled: bool,
    resources: GpuResourceStats,
    submissions: GpuSubmissionProgress,
    memory: Option<MemoryPoolStats>,
}

impl GpuDiagnosticSnapshot {
    /// Returns the active graphics backend without adapter names or identifiers.
    #[must_use]
    pub const fn backend(self) -> wgpu::Backend {
        self.backend
    }

    /// Returns the broad physical device class.
    #[must_use]
    pub const fn device_type(self) -> wgpu::DeviceType {
        self.device_type
    }

    /// Returns whether this logical device enabled managed pass timestamps.
    #[must_use]
    pub const fn timestamp_queries_enabled(self) -> bool {
        self.timestamp_queries_enabled
    }

    /// Returns exact aggregate live managed-resource counts.
    #[must_use]
    pub const fn resources(self) -> GpuResourceStats {
        self.resources
    }

    /// Returns exact aggregate queue submission and retirement progress.
    #[must_use]
    pub const fn submissions(self) -> GpuSubmissionProgress {
        self.submissions
    }

    /// Returns aggregate managed-memory accounting when supplied at capture.
    #[must_use]
    pub const fn memory(self) -> Option<MemoryPoolStats> {
        self.memory
    }

    /// Projects this snapshot into a user-safe shared diagnostic event.
    pub fn user_safe_event(self) -> Result<DiagnosticEvent> {
        let mut event = DiagnosticEvent::new(
            "gpu.snapshot",
            COMPONENT,
            DiagnosticSeverity::Debug,
            "GPU diagnostic snapshot captured",
        )?;
        insert_user_safe(&mut event, "adapter.backend", self.backend.to_str())?;
        insert_user_safe(
            &mut event,
            "adapter.device_type",
            device_type_code(self.device_type),
        )?;
        insert_user_safe(
            &mut event,
            "timing.timestamp_queries",
            self.timestamp_queries_enabled,
        )?;
        insert_user_safe(&mut event, "resources.total", self.resources.total())?;
        for kind in GpuResourceKind::ALL {
            insert_user_safe(
                &mut event,
                format!("resources.{}", kind.code()),
                self.resources.count(*kind),
            )?;
        }
        insert_user_safe(
            &mut event,
            "submissions.last_submitted",
            self.submissions.last_submitted(),
        )?;
        insert_user_safe(
            &mut event,
            "submissions.last_retired",
            self.submissions.last_retired(),
        )?;
        insert_user_safe(
            &mut event,
            "submissions.in_flight",
            self.submissions.in_flight(),
        )?;
        insert_user_safe(
            &mut event,
            "submissions.retained_resources",
            self.submissions.retained_resources(),
        )?;
        if let Some(memory) = self.memory {
            insert_user_safe(&mut event, "memory.resident_bytes", memory.resident_bytes())?;
            insert_user_safe(
                &mut event,
                "memory.peak_resident_bytes",
                memory.peak_resident_bytes(),
            )?;
            insert_user_safe(&mut event, "memory.pending_bytes", memory.pending_bytes())?;
            insert_user_safe(
                &mut event,
                "memory.denied_reservations",
                memory.denied_reservations(),
            )?;
            for class in MemoryClass::ALL {
                insert_user_safe(
                    &mut event,
                    format!("memory.{}_bytes", class.code()),
                    memory.resident_bytes_for(*class),
                )?;
            }
        }
        Ok(event)
    }
}

impl GpuResources<'_> {
    /// Captures aggregate diagnostics for this exact device and queue lifetime.
    pub fn diagnostic_snapshot(
        &self,
        submissions: &GpuSubmissionQueue<'_>,
        memory: Option<&GpuMemoryPool>,
    ) -> Result<GpuDiagnosticSnapshot> {
        submissions
            .ensure_device_identity(self.device_identity(), "capture_diagnostic_snapshot")?;
        let info = self.device().adapter().info();
        Ok(GpuDiagnosticSnapshot {
            backend: info.backend,
            device_type: info.device_type,
            timestamp_queries_enabled: self
                .enabled_features()
                .contains(wgpu::Features::TIMESTAMP_QUERY),
            resources: self.stats(),
            submissions: submissions.progress(),
            memory: memory.map(GpuMemoryPool::stats).transpose()?,
        })
    }
}

/// One privacy-safe managed pass duration.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GpuPassTiming {
    sequence: u64,
    kind: GpuPassKind,
    duration_nanoseconds: u64,
}

impl GpuPassTiming {
    /// Returns the pass order within the timed command buffer.
    #[must_use]
    pub const fn sequence(self) -> u64 {
        self.sequence
    }

    /// Returns whether the measured pass was compute or render work.
    #[must_use]
    pub const fn kind(self) -> GpuPassKind {
        self.kind
    }

    /// Returns elapsed GPU time rounded to the nearest nanosecond.
    #[must_use]
    pub const fn duration_nanoseconds(self) -> u64 {
        self.duration_nanoseconds
    }
}

/// Immutable privacy-safe timings for one submitted managed pass batch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuTimingReport {
    passes: Vec<GpuPassTiming>,
    total_nanoseconds: u64,
}

impl GpuTimingReport {
    /// Returns measured passes in exact command-buffer order.
    #[must_use]
    pub fn passes(&self) -> &[GpuPassTiming] {
        &self.passes
    }

    /// Returns the saturating sum of measured pass durations.
    #[must_use]
    pub const fn total_nanoseconds(&self) -> u64 {
        self.total_nanoseconds
    }

    /// Projects aggregate timing into a user-safe shared diagnostic event.
    pub fn user_safe_event(&self) -> Result<DiagnosticEvent> {
        let compute_passes = u64::try_from(
            self.passes
                .iter()
                .filter(|pass| pass.kind == GpuPassKind::Compute)
                .count(),
        )
        .map_err(|_| exhausted("project_timing_event", "compute pass count is exhausted"))?;
        let render_passes = u64::try_from(
            self.passes
                .iter()
                .filter(|pass| pass.kind == GpuPassKind::Render)
                .count(),
        )
        .map_err(|_| exhausted("project_timing_event", "render pass count is exhausted"))?;
        let pass_count = u64::try_from(self.passes.len())
            .map_err(|_| exhausted("project_timing_event", "pass count is exhausted"))?;
        let mut event = DiagnosticEvent::new(
            "gpu.timing.completed",
            COMPONENT,
            DiagnosticSeverity::Debug,
            "GPU pass timing completed",
        )?;
        insert_user_safe(&mut event, "passes.total", pass_count)?;
        insert_user_safe(&mut event, "passes.compute", compute_passes)?;
        insert_user_safe(&mut event, "passes.render", render_passes)?;
        insert_user_safe(
            &mut event,
            "timing.total_nanoseconds",
            self.total_nanoseconds,
        )?;
        Ok(event)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct GpuTimingTarget {
    sequence: u64,
    kind: GpuPassKind,
}

impl GpuTimingTarget {
    pub(crate) const fn new(sequence: u64, kind: GpuPassKind) -> Self {
        Self { sequence, kind }
    }
}

pub(crate) struct GpuTimingEncoder {
    query_set: wgpu::QuerySet,
    resolve: GpuBuffer,
    staging: GpuBuffer,
    max_passes: u32,
    recorded_passes: u32,
    timestamp_period: f32,
}

impl fmt::Debug for GpuTimingEncoder {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GpuTimingEncoder")
            .field("max_passes", &self.max_passes)
            .field("recorded_passes", &self.recorded_passes)
            .finish_non_exhaustive()
    }
}

impl GpuTimingEncoder {
    pub(crate) fn new(resources: &GpuResources<'_>, config: GpuTimingConfig) -> Result<Self> {
        if !resources
            .enabled_features()
            .contains(wgpu::Features::TIMESTAMP_QUERY)
        {
            return Err(unsupported(
                "create_timed_pass_encoder",
                "GPU pass timing requires TIMESTAMP_QUERY on the logical device",
            ));
        }
        let byte_count = u64::from(config.query_count())
            .checked_mul(u64::from(wgpu::QUERY_SIZE))
            .ok_or_else(|| {
                exhausted(
                    "create_timed_pass_encoder",
                    "timing buffer size is exhausted",
                )
            })?;
        if byte_count > resources.enabled_limits().max_buffer_size {
            return Err(exhausted(
                "create_timed_pass_encoder",
                "GPU timing buffers exceed the enabled device buffer limit",
            ));
        }
        let timestamp_period = resources.device().timestamp_period();
        if !timestamp_period.is_finite() || timestamp_period <= 0.0 {
            return Err(Error::new(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "wgpu returned an invalid GPU timestamp period",
            )
            .with_context(ErrorContext::new(COMPONENT, "create_timed_pass_encoder")));
        }
        let query_set = resources
            .wgpu_device()
            .create_query_set(&wgpu::QuerySetDescriptor {
                label: Some("superi-gpu pass timing queries"),
                ty: wgpu::QueryType::Timestamp,
                count: config.query_count(),
            });
        let resolve = resources.create_buffer(&wgpu::BufferDescriptor {
            label: Some("superi-gpu pass timing resolve"),
            size: byte_count,
            usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        })?;
        let staging = resources.create_buffer(&wgpu::BufferDescriptor {
            label: Some("superi-gpu pass timing staging"),
            size: byte_count,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        })?;
        Ok(Self {
            query_set,
            resolve,
            staging,
            max_passes: config.max_passes,
            recorded_passes: 0,
            timestamp_period,
        })
    }

    pub(crate) const fn query_set(&self) -> &wgpu::QuerySet {
        &self.query_set
    }

    pub(crate) fn reserve_pass(&mut self) -> Result<(u32, u32)> {
        if self.recorded_passes >= self.max_passes {
            return Err(exhausted(
                "encode_timed_pass",
                "GPU timing pass capacity is exhausted",
            ));
        }
        let beginning = self
            .recorded_passes
            .checked_mul(QUERIES_PER_PASS)
            .ok_or_else(|| exhausted("encode_timed_pass", "GPU timing query index is exhausted"))?;
        let end = beginning
            .checked_add(1)
            .ok_or_else(|| exhausted("encode_timed_pass", "GPU timing query index is exhausted"))?;
        self.recorded_passes = self
            .recorded_passes
            .checked_add(1)
            .ok_or_else(|| exhausted("encode_timed_pass", "GPU timing pass count is exhausted"))?;
        Ok((beginning, end))
    }

    pub(crate) fn finish(
        self,
        encoder: &mut wgpu::CommandEncoder,
        targets: Vec<GpuTimingTarget>,
    ) -> Result<EncodedGpuTiming> {
        let target_count = u32::try_from(targets.len()).map_err(|_| {
            exhausted(
                "finish_timed_pass_batch",
                "GPU timing target count is exhausted",
            )
        })?;
        if target_count != self.recorded_passes {
            return Err(Error::new(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "GPU timing targets do not match encoded passes",
            )
            .with_context(ErrorContext::new(COMPONENT, "finish_timed_pass_batch")));
        }
        let query_count = self
            .recorded_passes
            .checked_mul(QUERIES_PER_PASS)
            .ok_or_else(|| {
                exhausted(
                    "finish_timed_pass_batch",
                    "GPU timing query count is exhausted",
                )
            })?;
        let byte_count = u64::from(query_count)
            .checked_mul(u64::from(wgpu::QUERY_SIZE))
            .ok_or_else(|| {
                exhausted(
                    "finish_timed_pass_batch",
                    "GPU timing result size is exhausted",
                )
            })?;
        encoder.resolve_query_set(&self.query_set, 0..query_count, self.resolve.raw(), 0);
        encoder.copy_buffer_to_buffer(self.resolve.raw(), 0, self.staging.raw(), 0, byte_count);
        Ok(EncodedGpuTiming {
            _query_set: self.query_set,
            _resolve: self.resolve,
            staging: self.staging,
            targets,
            byte_count,
            timestamp_period: self.timestamp_period,
        })
    }
}

pub(crate) struct EncodedGpuTiming {
    _query_set: wgpu::QuerySet,
    _resolve: GpuBuffer,
    staging: GpuBuffer,
    targets: Vec<GpuTimingTarget>,
    byte_count: u64,
    timestamp_period: f32,
}

impl fmt::Debug for EncodedGpuTiming {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("EncodedGpuTiming")
            .field("pass_count", &self.targets.len())
            .field("byte_count", &self.byte_count)
            .finish_non_exhaustive()
    }
}

impl EncodedGpuTiming {
    pub(crate) fn pending_submission(&self) -> PendingGpuTiming {
        PendingGpuTiming {
            staging: self.staging.clone(),
            targets: self.targets.clone(),
            byte_count: self.byte_count,
            timestamp_period: self.timestamp_period,
        }
    }
}

pub(crate) struct PendingGpuTiming {
    staging: GpuBuffer,
    targets: Vec<GpuTimingTarget>,
    byte_count: u64,
    timestamp_period: f32,
}

impl PendingGpuTiming {
    pub(crate) fn begin(self, device_identity: Arc<()>, fence: GpuFence) -> GpuTimingHandle {
        let (sender, receiver) = mpsc::channel();
        self.staging
            .raw()
            .slice(..self.byte_count)
            .map_async(wgpu::MapMode::Read, move |result| {
                let _ = sender.send(result);
            });
        GpuTimingHandle {
            device_identity,
            fence,
            state: Arc::new(Mutex::new(GpuTimingState::Pending {
                staging: self.staging,
                targets: self.targets,
                byte_count: self.byte_count,
                timestamp_period: self.timestamp_period,
                receiver,
                mapping_ready: false,
                needs_unmap: true,
            })),
        }
    }
}

type MapResult = std::result::Result<(), wgpu::BufferAsyncError>;

enum GpuTimingState {
    Pending {
        staging: GpuBuffer,
        targets: Vec<GpuTimingTarget>,
        byte_count: u64,
        timestamp_period: f32,
        receiver: mpsc::Receiver<MapResult>,
        mapping_ready: bool,
        needs_unmap: bool,
    },
    Complete(GpuTimingReport),
    Failed,
}

impl GpuTimingState {
    const fn status(&self) -> &'static str {
        match self {
            Self::Pending { .. } => "pending",
            Self::Complete(_) => "complete",
            Self::Failed => "failed",
        }
    }

    fn pass_count(&self) -> usize {
        match self {
            Self::Pending { targets, .. } => targets.len(),
            Self::Complete(report) => report.passes.len(),
            Self::Failed => 0,
        }
    }
}

impl Drop for GpuTimingState {
    fn drop(&mut self) {
        if let Self::Pending {
            staging,
            needs_unmap: true,
            ..
        } = self
        {
            staging.raw().unmap();
        }
    }
}

/// Cloneable completion handle for one privacy-safe timed pass batch.
#[derive(Clone)]
#[must_use = "poll or wait for the GPU timing report"]
pub struct GpuTimingHandle {
    device_identity: Arc<()>,
    fence: GpuFence,
    state: Arc<Mutex<GpuTimingState>>,
}

impl fmt::Debug for GpuTimingHandle {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        let state = timing_state(&self.state);
        formatter
            .debug_struct("GpuTimingHandle")
            .field("fence", &self.fence)
            .field("status", &state.status())
            .field("pass_count", &state.pass_count())
            .finish()
    }
}

impl GpuTimingHandle {
    /// Returns the fence governing query and staging resource retirement.
    pub const fn fence(&self) -> &GpuFence {
        &self.fence
    }

    /// Polls once on the submission thread and returns an immutable report when ready.
    pub fn poll(&self, submissions: &GpuSubmissionQueue<'_>) -> Result<Option<GpuTimingReport>> {
        submissions.ensure_device_identity(&self.device_identity, "poll_gpu_timing")?;
        let progress = submissions.poll();
        let mut state = timing_state(&self.state);
        match &mut *state {
            GpuTimingState::Complete(report) => return Ok(Some(report.clone())),
            GpuTimingState::Failed => {
                return Err(Error::new(
                    ErrorCategory::Conflict,
                    Recoverability::UserCorrectable,
                    "GPU timing handle has already failed",
                )
                .with_context(ErrorContext::new(COMPONENT, "poll_gpu_timing")))
            }
            GpuTimingState::Pending { .. } => {}
        }

        let mapping_result = match &mut *state {
            GpuTimingState::Pending {
                receiver,
                mapping_ready,
                ..
            } if !*mapping_ready => match receiver.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    if let GpuTimingState::Pending { needs_unmap, .. } = &mut *state {
                        *needs_unmap = false;
                    }
                    *state = GpuTimingState::Failed;
                    return Err(Error::new(
                        ErrorCategory::Internal,
                        Recoverability::Terminal,
                        "GPU timing map callback disconnected",
                    )
                    .with_context(ErrorContext::new(COMPONENT, "map_gpu_timing")));
                }
            },
            _ => None,
        };
        if let Some(result) = mapping_result {
            match result {
                Ok(()) => {
                    if let GpuTimingState::Pending { mapping_ready, .. } = &mut *state {
                        *mapping_ready = true;
                    }
                }
                Err(source) => {
                    if let GpuTimingState::Pending { needs_unmap, .. } = &mut *state {
                        *needs_unmap = false;
                    }
                    *state = GpuTimingState::Failed;
                    return Err(Error::with_source(
                        ErrorCategory::Unavailable,
                        Recoverability::Retryable,
                        "GPU timing buffer mapping failed",
                        source,
                    )
                    .with_context(ErrorContext::new(COMPONENT, "map_gpu_timing")));
                }
            }
        }

        let ready = matches!(
            &*state,
            GpuTimingState::Pending {
                mapping_ready: true,
                ..
            }
        ) && progress.last_retired() >= self.fence.value();
        if !ready {
            return Ok(None);
        }

        let report = match &mut *state {
            GpuTimingState::Pending {
                staging,
                targets,
                byte_count,
                timestamp_period,
                needs_unmap,
                ..
            } => {
                let bytes = staging.raw().slice(..*byte_count).get_mapped_range();
                let report = build_report(&bytes, targets, *timestamp_period);
                drop(bytes);
                staging.raw().unmap();
                *needs_unmap = false;
                report
            }
            _ => unreachable!("ready timing state remains pending"),
        };
        match report {
            Ok(report) => {
                *state = GpuTimingState::Complete(report.clone());
                Ok(Some(report))
            }
            Err(error) => {
                *state = GpuTimingState::Failed;
                Err(error)
            }
        }
    }

    /// Waits for timing completion on the dedicated GPU submission thread.
    ///
    /// Use [`Self::poll`] from responsive event loops. This blocking helper must
    /// not run on UI, audio, playback, render coordinator, or job threads.
    pub fn wait(&self, submissions: &GpuSubmissionQueue<'_>) -> Result<GpuTimingReport> {
        loop {
            if let Some(report) = self.poll(submissions)? {
                return Ok(report);
            }
            std::thread::park_timeout(WAIT_INTERVAL);
        }
    }
}

fn build_report(
    bytes: &[u8],
    targets: &[GpuTimingTarget],
    timestamp_period: f32,
) -> Result<GpuTimingReport> {
    let expected = targets
        .len()
        .checked_mul(
            usize::try_from(QUERIES_PER_PASS * wgpu::QUERY_SIZE).map_err(|_| {
                exhausted(
                    "read_gpu_timing",
                    "GPU timing entry size does not fit host memory",
                )
            })?,
        )
        .ok_or_else(|| exhausted("read_gpu_timing", "GPU timing result size is exhausted"))?;
    if bytes.len() != expected {
        return Err(Error::new(
            ErrorCategory::CorruptData,
            Recoverability::Retryable,
            "GPU timing result has an unexpected size",
        )
        .with_context(ErrorContext::new(COMPONENT, "read_gpu_timing")));
    }

    let mut passes = Vec::with_capacity(targets.len());
    for (target, values) in targets.iter().zip(bytes.chunks_exact(16)) {
        let beginning = u64::from_ne_bytes(
            values[0..8]
                .try_into()
                .expect("timestamp beginning has eight bytes"),
        );
        let end = u64::from_ne_bytes(
            values[8..16]
                .try_into()
                .expect("timestamp end has eight bytes"),
        );
        let ticks = end.wrapping_sub(beginning);
        let nanoseconds = (ticks as f64) * f64::from(timestamp_period);
        if !nanoseconds.is_finite() || nanoseconds < 0.0 || nanoseconds > u64::MAX as f64 {
            return Err(Error::new(
                ErrorCategory::CorruptData,
                Recoverability::Retryable,
                "GPU timing duration is not representable",
            )
            .with_context(ErrorContext::new(COMPONENT, "read_gpu_timing")));
        }
        passes.push(GpuPassTiming {
            sequence: target.sequence,
            kind: target.kind,
            duration_nanoseconds: nanoseconds.round() as u64,
        });
    }
    let total_nanoseconds = passes.iter().fold(0_u64, |total, pass| {
        total.saturating_add(pass.duration_nanoseconds)
    });
    Ok(GpuTimingReport {
        passes,
        total_nanoseconds,
    })
}

fn timing_state(state: &Mutex<GpuTimingState>) -> MutexGuard<'_, GpuTimingState> {
    state
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

fn insert_user_safe(
    event: &mut DiagnosticEvent,
    name: impl Into<String>,
    value: impl Into<superi_core::diagnostics::TraceValue>,
) -> Result<()> {
    event.insert_field(name, TraceField::user_safe(value))?;
    Ok(())
}

const fn device_type_code(device_type: wgpu::DeviceType) -> &'static str {
    match device_type {
        wgpu::DeviceType::Other => "other",
        wgpu::DeviceType::IntegratedGpu => "integrated_gpu",
        wgpu::DeviceType::DiscreteGpu => "discrete_gpu",
        wgpu::DeviceType::VirtualGpu => "virtual_gpu",
        wgpu::DeviceType::Cpu => "cpu",
    }
}

fn invalid(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
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

fn exhausted(operation: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::ResourceExhausted,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation))
}
