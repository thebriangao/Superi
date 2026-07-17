//! Deterministic native audio plugin discovery, worker supervision, and project checkpoints.
//!
//! Discovery never maps plugin code. A caller-owned launcher must place scanning and processing in
//! a bounded worker process, while this supervisor owns identity validation, lifecycle, retained
//! checkpoints, quarantine, recovery, and conversion to the project extension envelope.

use std::collections::{BTreeMap, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use superi_audio::graph::AudioNodeId;
use superi_audio::hosting::AudioUnitComponentId;
use superi_audio::plugins::{
    AudioPluginFormat, AudioPluginIdentity, AudioPluginRuntimeReadings, AudioPluginState,
    IsolatedAudioPluginProcessBridge, PreparedIsolatedAudioPlugin,
    MAX_AUDIO_PLUGIN_STATE_TOTAL_BYTES,
};
use superi_concurrency::threads::ExecutionDomain;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::pixel::ChannelLayout;
use superi_core::settings::{CapabilitySet, ComponentId, SemanticVersion, VersionIdentifier};
use superi_project::extensions::{
    ProjectExtensionCommand, ProjectExtensionKey, ProjectExtensionKind, ProjectExtensionLifecycle,
    ProjectExtensionRecord, ProjectExtensionRecordId,
};

const COMPONENT: &str = "superi-engine.audio-plugins";
const VST3_SUFFIX: &str = ".vst3";
const MAX_DISCOVERY_DEPTH: usize = 64;
const MAX_DESCRIPTOR_NAME_BYTES: usize = 512;
const MAX_SUPPORTED_LAYOUTS: usize = 64;
const MAX_WORKER_MESSAGE_BYTES: usize = MAX_AUDIO_PLUGIN_STATE_TOTAL_BYTES + 64 * 1024;
const MAX_WORKER_ACTION_DEADLINE_MILLIS: u64 = 300_000;
const PROJECT_EXTENSION_ID: &str = "superi.audio-plugin-host";
const PROJECT_PAYLOAD_ID: &str = "superi.audio-plugin-state";
const PROJECT_RECORD_PREFIX: &str = "audio-node-";

/// Stable project identity for one authored audio plugin node instance.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct AudioPluginInstanceId(u128);

impl AudioPluginInstanceId {
    /// Derives plugin-instance identity from the authored audio graph node.
    #[must_use]
    pub const fn from_audio_node(node: AudioNodeId) -> Self {
        Self(node.raw())
    }

    /// Creates an identity from its opaque persisted value.
    #[must_use]
    pub const fn from_raw(raw: u128) -> Self {
        Self(raw)
    }

    /// Returns the opaque persisted value.
    #[must_use]
    pub const fn raw(self) -> u128 {
        self.0
    }
}

/// One validated native plugin candidate discovered without mapping executable code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioPluginCandidate {
    format: AudioPluginFormat,
    source: String,
    vst3_bundle: Option<PathBuf>,
    audio_unit: Option<AudioUnitComponentId>,
}

impl AudioPluginCandidate {
    /// Creates one Audio Unit candidate from an operating-system component enumeration result.
    #[must_use]
    pub fn audio_unit(component: AudioUnitComponentId) -> Self {
        let source = format!(
            "audio-unit:{:08X}:{:08X}:{:08X}",
            component.raw_component_type(),
            component.raw_subtype(),
            component.raw_manufacturer()
        );
        Self {
            format: AudioPluginFormat::AudioUnit,
            source,
            vst3_bundle: None,
            audio_unit: Some(component),
        }
    }

    fn vst3(path: PathBuf) -> Result<Self> {
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                candidate_error(
                    ErrorCategory::InvalidInput,
                    "validate_vst3_bundle",
                    &path,
                    "VST3 bundle name must be valid UTF-8",
                )
            })?;
        if name.len() <= VST3_SUFFIX.len() || !name.ends_with(VST3_SUFFIX) {
            return Err(candidate_error(
                ErrorCategory::InvalidInput,
                "validate_vst3_bundle",
                &path,
                "VST3 bundle name must end in .vst3",
            ));
        }
        let metadata = fs::symlink_metadata(&path)
            .map_err(|source| filesystem_error("inspect_vst3_bundle", &path, source))?;
        if metadata.file_type().is_symlink() {
            return Err(candidate_error(
                ErrorCategory::PermissionDenied,
                "validate_vst3_bundle",
                &path,
                "VST3 bundle path cannot be a symbolic link",
            ));
        }
        if !metadata.is_dir() {
            return Err(candidate_error(
                ErrorCategory::InvalidInput,
                "validate_vst3_bundle",
                &path,
                "VST3 bundle path must be a directory",
            ));
        }
        let contents = path.join("Contents");
        let contents_metadata = fs::symlink_metadata(&contents)
            .map_err(|source| filesystem_error("inspect_vst3_contents", &contents, source))?;
        if contents_metadata.file_type().is_symlink() || !contents_metadata.is_dir() {
            return Err(candidate_error(
                ErrorCategory::InvalidInput,
                "validate_vst3_bundle",
                &path,
                "VST3 bundle Contents entry must be a real directory",
            ));
        }
        let path = fs::canonicalize(&path)
            .map_err(|source| filesystem_error("canonicalize_vst3_bundle", &path, source))?;
        Ok(Self {
            format: AudioPluginFormat::Vst3,
            source: path.display().to_string(),
            vst3_bundle: Some(path),
            audio_unit: None,
        })
    }

    /// Returns the native plugin format.
    #[must_use]
    pub const fn format(&self) -> AudioPluginFormat {
        self.format
    }

    /// Returns the stable component or canonical bundle source used for diagnostics.
    #[must_use]
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns the canonical VST3 package path when this is a VST3 candidate.
    #[must_use]
    pub fn vst3_bundle(&self) -> Option<&Path> {
        self.vst3_bundle.as_deref()
    }

    /// Returns the exact Audio Unit component when this is an Audio Unit candidate.
    #[must_use]
    pub const fn audio_unit_component(&self) -> Option<AudioUnitComponentId> {
        self.audio_unit
    }
}

/// Contained filesystem, validation, scan, or worker failure evidence.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioPluginFailure {
    stage: String,
    source: Option<String>,
    category: ErrorCategory,
    recoverability: Recoverability,
    message: String,
    contexts: Vec<ErrorContext>,
    total_failures: u64,
    consecutive_failures: u32,
}

impl AudioPluginFailure {
    fn from_error(
        stage: impl Into<String>,
        source: Option<String>,
        error: &Error,
        total_failures: u64,
        consecutive_failures: u32,
    ) -> Self {
        Self {
            stage: stage.into(),
            source,
            category: error.category(),
            recoverability: error.recoverability(),
            message: error.message().to_owned(),
            contexts: error.contexts().to_vec(),
            total_failures,
            consecutive_failures,
        }
    }

    /// Returns the stable failing stage.
    #[must_use]
    pub fn stage(&self) -> &str {
        &self.stage
    }

    /// Returns the candidate or exact plugin source.
    #[must_use]
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// Returns the shared failure category.
    #[must_use]
    pub const fn category(&self) -> ErrorCategory {
        self.category
    }

    /// Returns the shared recovery classification.
    #[must_use]
    pub const fn recoverability(&self) -> Recoverability {
        self.recoverability
    }

    /// Returns the retained diagnostic message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns retained structured contexts.
    #[must_use]
    pub fn contexts(&self) -> &[ErrorContext] {
        &self.contexts
    }

    /// Returns the lifetime worker failure count.
    #[must_use]
    pub const fn total_failures(&self) -> u64 {
        self.total_failures
    }

    /// Returns failures since the latest healthy activation.
    #[must_use]
    pub const fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }
}

/// Deterministic candidates plus contained discovery failures.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct AudioPluginDiscoveryReport {
    candidates: Vec<AudioPluginCandidate>,
    failures: Vec<AudioPluginFailure>,
}

impl AudioPluginDiscoveryReport {
    /// Creates a report from an explicit Audio Unit component enumeration.
    #[must_use]
    pub fn audio_units(components: impl IntoIterator<Item = AudioUnitComponentId>) -> Self {
        let mut candidates = components
            .into_iter()
            .map(AudioPluginCandidate::audio_unit)
            .collect::<Vec<_>>();
        candidates.sort_by(|left, right| left.source.cmp(&right.source));
        candidates.dedup_by(|left, right| left.source == right.source);
        Self {
            candidates,
            failures: Vec::new(),
        }
    }

    /// Merges reports while retaining deterministic first-seen sources and all failures.
    #[must_use]
    pub fn merge(mut self, other: Self) -> Self {
        self.candidates.extend(other.candidates);
        self.candidates
            .sort_by(|left, right| left.source.cmp(&right.source));
        self.candidates
            .dedup_by(|left, right| left.source == right.source);
        self.failures.extend(other.failures);
        self
    }

    /// Returns candidates in canonical source order.
    #[must_use]
    pub fn candidates(&self) -> &[AudioPluginCandidate] {
        &self.candidates
    }

    /// Returns contained discovery failures.
    #[must_use]
    pub fn failures(&self) -> &[AudioPluginFailure] {
        &self.failures
    }
}

/// Recursively discovers VST3 packages without loading their native modules.
pub fn discover_vst3_bundles<I, P>(roots: I) -> Result<AudioPluginDiscoveryReport>
where
    I: IntoIterator<Item = P>,
    P: Into<PathBuf>,
{
    require_domain(ExecutionDomain::BackgroundJob, "discover")?;
    let mut candidates = BTreeMap::new();
    let mut failures = Vec::new();
    let mut queue = roots
        .into_iter()
        .map(|root| (root.into(), 0_usize))
        .collect::<VecDeque<_>>();
    while let Some((path, depth)) = queue.pop_front() {
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(source) => {
                let error = filesystem_error("discover", &path, source);
                failures.push(AudioPluginFailure::from_error(
                    "discover",
                    Some(path.display().to_string()),
                    &error,
                    1,
                    1,
                ));
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            continue;
        }
        if !metadata.is_dir() {
            continue;
        }
        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.ends_with(VST3_SUFFIX))
        {
            match AudioPluginCandidate::vst3(path.clone()) {
                Ok(candidate) => {
                    candidates.insert(candidate.source.clone(), candidate);
                }
                Err(error) => failures.push(AudioPluginFailure::from_error(
                    "validate_bundle",
                    Some(path.display().to_string()),
                    &error,
                    1,
                    1,
                )),
            }
            continue;
        }
        if depth >= MAX_DISCOVERY_DEPTH {
            let error = plugin_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::Degraded,
                "discover",
                "VST3 discovery reached its bounded recursion depth",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "discover")
                    .with_field("path", path.display().to_string()),
            );
            failures.push(AudioPluginFailure::from_error(
                "discover",
                Some(path.display().to_string()),
                &error,
                1,
                1,
            ));
            continue;
        }
        let entries = match fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(source) => {
                let error = filesystem_error("read_search_directory", &path, source);
                failures.push(AudioPluginFailure::from_error(
                    "discover",
                    Some(path.display().to_string()),
                    &error,
                    1,
                    1,
                ));
                continue;
            }
        };
        let mut children = Vec::new();
        for entry in entries {
            match entry {
                Ok(entry) => {
                    let child = entry.path();
                    if !child
                        .file_name()
                        .is_some_and(|name| name.to_string_lossy().starts_with('@'))
                    {
                        children.push(child);
                    }
                }
                Err(source) => {
                    let error = filesystem_error("read_search_entry", &path, source);
                    failures.push(AudioPluginFailure::from_error(
                        "discover",
                        Some(path.display().to_string()),
                        &error,
                        1,
                        1,
                    ));
                }
            }
        }
        children.sort();
        queue.extend(children.into_iter().map(|child| (child, depth + 1)));
    }
    Ok(AudioPluginDiscoveryReport {
        candidates: candidates.into_values().collect(),
        failures,
    })
}

/// Native-code placement promised by one audio plugin adapter.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum AudioPluginWorkerIsolation {
    /// Native code is confined to a restartable child process.
    WorkerProcess,
    /// Native code would enter the editor process and is rejected.
    InProcess,
}

/// Bounded worker protocol contract validated before scanning any native plugin.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioPluginWorkerContract {
    isolation: AudioPluginWorkerIsolation,
    protocol_version: u32,
    maximum_message_bytes: usize,
    action_deadline_millis: u64,
    restartable: bool,
    fixed_transport_latency_samples: usize,
}

impl AudioPluginWorkerContract {
    /// Creates a contract only when isolation, bounds, deadlines, and recovery are explicit.
    pub fn new(
        isolation: AudioPluginWorkerIsolation,
        protocol_version: u32,
        maximum_message_bytes: usize,
        action_deadline_millis: u64,
        restartable: bool,
        fixed_transport_latency_samples: usize,
    ) -> Result<Self> {
        if isolation != AudioPluginWorkerIsolation::WorkerProcess {
            return Err(plugin_error(
                ErrorCategory::PermissionDenied,
                Recoverability::Terminal,
                "validate_worker_contract",
                "native audio plugin code must remain outside the editor process",
            ));
        }
        if protocol_version == 0
            || maximum_message_bytes == 0
            || maximum_message_bytes > MAX_WORKER_MESSAGE_BYTES
            || action_deadline_millis == 0
            || action_deadline_millis > MAX_WORKER_ACTION_DEADLINE_MILLIS
            || !restartable
        {
            return Err(plugin_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "validate_worker_contract",
                "audio plugin worker contract is unbounded or cannot recover",
            ));
        }
        Ok(Self {
            isolation,
            protocol_version,
            maximum_message_bytes,
            action_deadline_millis,
            restartable,
            fixed_transport_latency_samples,
        })
    }

    /// Returns the process-isolation mode.
    #[must_use]
    pub const fn isolation(self) -> AudioPluginWorkerIsolation {
        self.isolation
    }

    /// Returns the bounded IPC protocol version.
    #[must_use]
    pub const fn protocol_version(self) -> u32 {
        self.protocol_version
    }

    /// Returns the maximum encoded control message size.
    #[must_use]
    pub const fn maximum_message_bytes(self) -> usize {
        self.maximum_message_bytes
    }

    /// Returns the control action deadline.
    #[must_use]
    pub const fn action_deadline_millis(self) -> u64 {
        self.action_deadline_millis
    }

    /// Returns whether a fresh worker generation can be launched.
    #[must_use]
    pub const fn is_restartable(self) -> bool {
        self.restartable
    }

    /// Returns fixed transport delay in sample frames.
    #[must_use]
    pub const fn fixed_transport_latency_samples(self) -> usize {
        self.fixed_transport_latency_samples
    }
}

/// Validated plugin metadata returned by an isolated scan worker.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioPluginDescriptor {
    identity: AudioPluginIdentity,
    name: String,
    latency_samples: usize,
    supported_layouts: Vec<ChannelLayout>,
}

impl AudioPluginDescriptor {
    /// Creates bounded scan metadata for one exact plugin identity.
    pub fn new(
        identity: AudioPluginIdentity,
        name: impl Into<String>,
        latency_samples: usize,
        supported_layouts: impl IntoIterator<Item = ChannelLayout>,
    ) -> Result<Self> {
        let name = name.into();
        if name.is_empty()
            || name.len() > MAX_DESCRIPTOR_NAME_BYTES
            || name.chars().any(char::is_control)
        {
            return Err(plugin_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "validate_descriptor",
                "audio plugin display name is empty, invalid, or exceeds its bound",
            ));
        }
        let mut layouts = supported_layouts.into_iter().collect::<Vec<_>>();
        if layouts.is_empty() || layouts.len() > MAX_SUPPORTED_LAYOUTS {
            return Err(plugin_error(
                ErrorCategory::Unsupported,
                Recoverability::UserCorrectable,
                "validate_descriptor",
                "audio plugin layout set is empty or exceeds its explicit bound",
            ));
        }
        let mut unique = Vec::with_capacity(layouts.len());
        for layout in layouts.drain(..) {
            if !unique.contains(&layout) {
                unique.push(layout);
            }
        }
        Ok(Self {
            identity,
            name,
            latency_samples,
            supported_layouts: unique,
        })
    }

    /// Returns the exact format-specific identity.
    #[must_use]
    pub const fn identity(&self) -> &AudioPluginIdentity {
        &self.identity
    }

    /// Returns the scanner-provided display name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the native algorithmic latency observed by the worker.
    #[must_use]
    pub const fn latency_samples(&self) -> usize {
        self.latency_samples
    }

    /// Returns exact layouts accepted by this scanner result.
    #[must_use]
    pub fn supported_layouts(&self) -> &[ChannelLayout] {
        &self.supported_layouts
    }

    /// Returns whether one exact semantic layout is supported.
    #[must_use]
    pub fn supports_layout(&self, layout: &ChannelLayout) -> bool {
        self.supported_layouts.contains(layout)
    }
}

/// Isolated control and process adapter for one discovered plugin candidate.
pub trait AudioPluginWorkerAdapter {
    /// Returns the already validated worker contract.
    fn contract(&self) -> AudioPluginWorkerContract;
    /// Scans and validates the candidate without activating it for project work.
    fn scan(&mut self) -> Result<AudioPluginDescriptor>;
    /// Activates the plugin, optionally restoring one exact checkpoint.
    fn activate(&mut self, state: Option<&AudioPluginState>) -> Result<()>;
    /// Captures one exact checkpoint outside the audio callback.
    fn capture_state(&mut self) -> Result<AudioPluginState>;
    /// Creates the nonblocking process-side bridge for one prepared graph node.
    fn prepare_process_bridge(
        &mut self,
        sample_rate: u32,
        layout: &ChannelLayout,
        maximum_frames: usize,
    ) -> Result<Box<dyn IsolatedAudioPluginProcessBridge>>;
    /// Deactivates the current worker instance.
    fn deactivate(&mut self) -> Result<()>;
    /// Starts a clean worker generation after a contained failure.
    fn restart_worker(&mut self) -> Result<()>;
}

/// Platform-owned launcher that applies sandboxing before native scan.
pub trait AudioPluginWorkerLauncher {
    /// Launches one isolated adapter for a validated candidate.
    fn launch(
        &mut self,
        candidate: &AudioPluginCandidate,
    ) -> Result<Box<dyn AudioPluginWorkerAdapter>>;
}

impl<F> AudioPluginWorkerLauncher for F
where
    F: FnMut(&AudioPluginCandidate) -> Result<Box<dyn AudioPluginWorkerAdapter>>,
{
    fn launch(
        &mut self,
        candidate: &AudioPluginCandidate,
    ) -> Result<Box<dyn AudioPluginWorkerAdapter>> {
        self(candidate)
    }
}

/// Engine-owned lifecycle for one accepted native audio plugin.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum AudioPluginLifecycle {
    /// Scanned but not active for project work.
    Disabled,
    /// Active and available for graph preparation.
    Ready,
    /// Current worker generation failed and can be restarted.
    Faulted,
    /// Repeated failures require explicit user acknowledgement.
    Quarantined,
}

struct ManagedPlugin {
    source: String,
    descriptor: AudioPluginDescriptor,
    contract: AudioPluginWorkerContract,
    adapter: Box<dyn AudioPluginWorkerAdapter>,
    lifecycle: AudioPluginLifecycle,
    total_failures: u64,
    consecutive_failures: u32,
    last_failure: Option<AudioPluginFailure>,
    checkpoint: Option<AudioPluginState>,
}

/// Immutable user-inspectable state for one accepted plugin.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AudioPluginStatusSnapshot {
    descriptor: AudioPluginDescriptor,
    lifecycle: AudioPluginLifecycle,
    total_failures: u64,
    consecutive_failures: u32,
    last_failure: Option<AudioPluginFailure>,
    has_checkpoint: bool,
}

impl AudioPluginStatusSnapshot {
    /// Returns the isolated scanner result.
    #[must_use]
    pub const fn descriptor(&self) -> &AudioPluginDescriptor {
        &self.descriptor
    }

    /// Returns the current supervised lifecycle.
    #[must_use]
    pub const fn lifecycle(&self) -> AudioPluginLifecycle {
        self.lifecycle
    }

    /// Returns the lifetime contained failure count.
    #[must_use]
    pub const fn total_failures(&self) -> u64 {
        self.total_failures
    }

    /// Returns failures since the latest healthy activation.
    #[must_use]
    pub const fn consecutive_failures(&self) -> u32 {
        self.consecutive_failures
    }

    /// Returns the latest safe-to-project engine failure record.
    #[must_use]
    pub const fn last_failure(&self) -> Option<&AudioPluginFailure> {
        self.last_failure.as_ref()
    }

    /// Returns whether exact state is available for recovery and project save.
    #[must_use]
    pub const fn has_checkpoint(&self) -> bool {
        self.has_checkpoint
    }
}

/// One deterministic scan, lifecycle, recovery, and project-checkpoint coordinator.
pub struct AudioPluginSupervisor {
    plugins: BTreeMap<AudioPluginIdentity, ManagedPlugin>,
    retained_failures: Vec<AudioPluginFailure>,
    quarantine_threshold: u32,
    revision: u64,
}

impl AudioPluginSupervisor {
    /// Launches isolated scanners, validates identities, and indexes every healthy candidate.
    pub fn scan(
        discovery: AudioPluginDiscoveryReport,
        launcher: &mut impl AudioPluginWorkerLauncher,
        quarantine_threshold: u32,
    ) -> Result<Self> {
        require_domain(ExecutionDomain::BackgroundJob, "scan")?;
        if quarantine_threshold == 0 {
            return Err(plugin_error(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "scan",
                "audio plugin quarantine threshold must be positive",
            ));
        }
        let AudioPluginDiscoveryReport {
            candidates,
            mut failures,
        } = discovery;
        let mut plugins = BTreeMap::new();
        for candidate in candidates {
            let source = candidate.source.clone();
            let mut adapter = match launcher.launch(&candidate) {
                Ok(adapter) => adapter,
                Err(error) => {
                    failures.push(AudioPluginFailure::from_error(
                        "launch",
                        Some(source),
                        &error,
                        1,
                        1,
                    ));
                    continue;
                }
            };
            let contract = adapter.contract();
            if contract.isolation() != AudioPluginWorkerIsolation::WorkerProcess
                || !contract.is_restartable()
                || contract.protocol_version() == 0
                || contract.maximum_message_bytes() == 0
                || contract.action_deadline_millis() == 0
            {
                let error = plugin_error(
                    ErrorCategory::PermissionDenied,
                    Recoverability::Terminal,
                    "validate_worker_contract",
                    "audio plugin adapter did not preserve its isolated bounded contract",
                );
                failures.push(AudioPluginFailure::from_error(
                    "validate_worker_contract",
                    Some(source),
                    &error,
                    1,
                    1,
                ));
                continue;
            }
            let descriptor = match adapter.scan() {
                Ok(descriptor) => descriptor,
                Err(error) => {
                    failures.push(AudioPluginFailure::from_error(
                        "scan",
                        Some(source),
                        &error,
                        1,
                        1,
                    ));
                    continue;
                }
            };
            if descriptor.identity().format() != candidate.format() {
                let error = plugin_error(
                    ErrorCategory::Conflict,
                    Recoverability::Terminal,
                    "validate_identity",
                    "audio plugin scan format does not match its candidate",
                );
                failures.push(AudioPluginFailure::from_error(
                    "validate_identity",
                    Some(source),
                    &error,
                    1,
                    1,
                ));
                continue;
            }
            let identity = descriptor.identity().clone();
            if plugins.contains_key(&identity) {
                let error = plugin_error(
                    ErrorCategory::Conflict,
                    Recoverability::UserCorrectable,
                    "validate_identity",
                    "multiple audio plugin candidates expose the same exact identity",
                );
                failures.push(AudioPluginFailure::from_error(
                    "validate_identity",
                    Some(source),
                    &error,
                    1,
                    1,
                ));
                continue;
            }
            plugins.insert(
                identity,
                ManagedPlugin {
                    source,
                    descriptor,
                    contract,
                    adapter,
                    lifecycle: AudioPluginLifecycle::Disabled,
                    total_failures: 0,
                    consecutive_failures: 0,
                    last_failure: None,
                    checkpoint: None,
                },
            );
        }
        Ok(Self {
            plugins,
            retained_failures: failures,
            quarantine_threshold,
            revision: 1,
        })
    }

    /// Returns the monotonic supervisor state revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the number of accepted exact plugin identities.
    #[must_use]
    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    /// Returns one immutable lifecycle snapshot.
    #[must_use]
    pub fn status(&self, identity: &AudioPluginIdentity) -> Option<AudioPluginStatusSnapshot> {
        let plugin = self.plugins.get(identity)?;
        Some(AudioPluginStatusSnapshot {
            descriptor: plugin.descriptor.clone(),
            lifecycle: plugin.lifecycle,
            total_failures: plugin.total_failures,
            consecutive_failures: plugin.consecutive_failures,
            last_failure: plugin.last_failure.clone(),
            has_checkpoint: plugin.checkpoint.is_some(),
        })
    }

    /// Returns every accepted native plugin status in exact identity order.
    #[must_use]
    pub fn statuses(&self) -> Vec<AudioPluginStatusSnapshot> {
        self.plugins
            .values()
            .map(|plugin| AudioPluginStatusSnapshot {
                descriptor: plugin.descriptor.clone(),
                lifecycle: plugin.lifecycle,
                total_failures: plugin.total_failures,
                consecutive_failures: plugin.consecutive_failures,
                last_failure: plugin.last_failure.clone(),
                has_checkpoint: plugin.checkpoint.is_some(),
            })
            .collect()
    }

    /// Returns discovery failures plus each accepted plugin's latest worker failure.
    #[must_use]
    pub fn failures(&self) -> Vec<AudioPluginFailure> {
        let mut failures = self.retained_failures.clone();
        failures.extend(
            self.plugins
                .values()
                .filter_map(|plugin| plugin.last_failure.clone()),
        );
        failures
    }

    /// Activates one plugin and restores an optional exact checkpoint.
    pub fn enable(
        &mut self,
        identity: &AudioPluginIdentity,
        state: Option<AudioPluginState>,
    ) -> Result<()> {
        require_domain(ExecutionDomain::BackgroundJob, "enable")?;
        if state
            .as_ref()
            .is_some_and(|state| !state.identity().is_same_component(identity))
        {
            return Err(plugin_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "enable",
                "audio plugin checkpoint identity does not match the requested plugin",
            ));
        }
        let threshold = self.quarantine_threshold;
        let plugin = self.plugin_mut(identity, "enable")?;
        match plugin.lifecycle {
            AudioPluginLifecycle::Disabled => {}
            AudioPluginLifecycle::Ready => {
                return Err(plugin_error(
                    ErrorCategory::Conflict,
                    Recoverability::UserCorrectable,
                    "enable",
                    "audio plugin is already enabled",
                ));
            }
            AudioPluginLifecycle::Faulted => {
                return Err(plugin_error(
                    ErrorCategory::Conflict,
                    Recoverability::Retryable,
                    "enable",
                    "faulted audio plugin must use supervised recovery",
                ));
            }
            AudioPluginLifecycle::Quarantined => {
                return Err(plugin_error(
                    ErrorCategory::PermissionDenied,
                    Recoverability::UserCorrectable,
                    "enable",
                    "quarantined audio plugin requires explicit acknowledgement",
                ));
            }
        }
        if let Some(state) = state.as_ref() {
            plugin.checkpoint = Some(state.clone());
        }
        match plugin.adapter.activate(state.as_ref()) {
            Ok(()) => {
                mark_healthy(plugin);
                self.advance_revision()?;
                Ok(())
            }
            Err(error) => {
                record_failure(plugin, "activate", &error, threshold);
                self.advance_revision()?;
                Err(error)
            }
        }
    }

    /// Captures and retains exact native state for project save and worker recovery.
    pub fn capture_checkpoint(
        &mut self,
        identity: &AudioPluginIdentity,
    ) -> Result<AudioPluginState> {
        require_domain(ExecutionDomain::BackgroundJob, "capture_checkpoint")?;
        let threshold = self.quarantine_threshold;
        let plugin = self.plugin_mut(identity, "capture_checkpoint")?;
        if plugin.lifecycle != AudioPluginLifecycle::Ready {
            return Err(plugin_error(
                ErrorCategory::Unavailable,
                Recoverability::Retryable,
                "capture_checkpoint",
                "audio plugin must be ready before state capture",
            ));
        }
        match plugin.adapter.capture_state() {
            Ok(state)
                if state.identity() == identity
                    && state.native_latency_samples() == plugin.descriptor.latency_samples()
                    && state.transport_latency_samples()
                        == plugin.contract.fixed_transport_latency_samples() =>
            {
                plugin.checkpoint = Some(state.clone());
                self.advance_revision()?;
                Ok(state)
            }
            Ok(_) => {
                let error = plugin_error(
                    ErrorCategory::Conflict,
                    Recoverability::UserCorrectable,
                    "capture_checkpoint",
                    "captured audio plugin identity or latency changed and requires rescan",
                );
                record_failure(plugin, "capture_state", &error, threshold);
                self.advance_revision()?;
                Err(error)
            }
            Err(error) => {
                record_failure(plugin, "capture_state", &error, threshold);
                self.advance_revision()?;
                Err(error)
            }
        }
    }

    /// Restarts one faulted generation and restores its most recent checkpoint.
    pub fn recover(&mut self, identity: &AudioPluginIdentity) -> Result<()> {
        require_domain(ExecutionDomain::BackgroundJob, "recover")?;
        let threshold = self.quarantine_threshold;
        let plugin = self.plugin_mut(identity, "recover")?;
        if plugin.lifecycle == AudioPluginLifecycle::Quarantined {
            return Err(plugin_error(
                ErrorCategory::PermissionDenied,
                Recoverability::UserCorrectable,
                "recover",
                "quarantined audio plugin requires explicit acknowledgement",
            ));
        }
        if plugin.lifecycle != AudioPluginLifecycle::Faulted {
            return Err(plugin_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "recover",
                "only a faulted audio plugin generation can be recovered",
            ));
        }
        let result = plugin
            .adapter
            .restart_worker()
            .and_then(|()| plugin.adapter.activate(plugin.checkpoint.as_ref()));
        match result {
            Ok(()) => {
                mark_healthy(plugin);
                self.advance_revision()?;
                Ok(())
            }
            Err(error) => {
                record_failure(plugin, "recover", &error, threshold);
                self.advance_revision()?;
                Err(error)
            }
        }
    }

    /// Acknowledges quarantine and starts a fresh generation from the retained checkpoint.
    pub fn clear_quarantine(&mut self, identity: &AudioPluginIdentity) -> Result<()> {
        require_domain(ExecutionDomain::BackgroundJob, "clear_quarantine")?;
        let plugin = self.plugin_mut(identity, "clear_quarantine")?;
        if plugin.lifecycle != AudioPluginLifecycle::Quarantined {
            return Err(plugin_error(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "clear_quarantine",
                "audio plugin is not quarantined",
            ));
        }
        plugin.consecutive_failures = 0;
        plugin.lifecycle = AudioPluginLifecycle::Faulted;
        self.recover(identity)
    }

    /// Deactivates one ready or faulted plugin without deleting its checkpoint.
    pub fn disable(&mut self, identity: &AudioPluginIdentity) -> Result<()> {
        require_domain(ExecutionDomain::BackgroundJob, "disable")?;
        let threshold = self.quarantine_threshold;
        let plugin = self.plugin_mut(identity, "disable")?;
        match plugin.adapter.deactivate() {
            Ok(()) => {
                plugin.lifecycle = AudioPluginLifecycle::Disabled;
                plugin.consecutive_failures = 0;
                plugin.last_failure = None;
                self.advance_revision()?;
                Ok(())
            }
            Err(error) => {
                record_failure(plugin, "deactivate", &error, threshold);
                self.advance_revision()?;
                Err(error)
            }
        }
    }

    /// Creates one graph processor whose worker faults fall back to latency-matched dry audio.
    pub fn prepare_processor(
        &mut self,
        identity: &AudioPluginIdentity,
        sample_rate: u32,
        layout: ChannelLayout,
        maximum_frames: usize,
    ) -> Result<(PreparedIsolatedAudioPlugin, AudioPluginRuntimeReadings)> {
        require_domain(ExecutionDomain::BackgroundJob, "prepare_processor")?;
        let threshold = self.quarantine_threshold;
        let plugin = self.plugin_mut(identity, "prepare_processor")?;
        if plugin.lifecycle != AudioPluginLifecycle::Ready {
            return Err(plugin_error(
                ErrorCategory::Unavailable,
                Recoverability::Degraded,
                "prepare_processor",
                "audio plugin is not ready for graph preparation",
            ));
        }
        if !plugin.descriptor.supports_layout(&layout) {
            return Err(plugin_error(
                ErrorCategory::Unsupported,
                Recoverability::UserCorrectable,
                "prepare_processor",
                "audio plugin does not support the requested semantic layout",
            ));
        }
        let result = plugin
            .adapter
            .prepare_process_bridge(sample_rate, &layout, maximum_frames)
            .and_then(|bridge| {
                if bridge.fixed_transport_latency_samples()
                    != plugin.contract.fixed_transport_latency_samples()
                {
                    return Err(plugin_error(
                        ErrorCategory::Conflict,
                        Recoverability::Terminal,
                        "prepare_processor",
                        "audio plugin process bridge changed its fixed transport latency",
                    ));
                }
                PreparedIsolatedAudioPlugin::new(
                    bridge,
                    sample_rate,
                    layout,
                    maximum_frames,
                    plugin.descriptor.latency_samples(),
                )
            });
        match result {
            Ok(prepared) => Ok(prepared),
            Err(error) => {
                record_failure(plugin, "prepare_processor", &error, threshold);
                self.advance_revision()?;
                Err(error)
            }
        }
    }

    /// Records a real-time worker fault observed through cloneable processor readings.
    pub fn observe_runtime(
        &mut self,
        identity: &AudioPluginIdentity,
        readings: &AudioPluginRuntimeReadings,
    ) -> Result<()> {
        require_domain(ExecutionDomain::EngineControl, "observe_runtime")?;
        let snapshot = readings.snapshot();
        if !snapshot.is_faulted() {
            return Ok(());
        }
        let threshold = self.quarantine_threshold;
        let plugin = self.plugin_mut(identity, "observe_runtime")?;
        if plugin.lifecycle != AudioPluginLifecycle::Ready {
            return Ok(());
        }
        let error = plugin_error(
            ErrorCategory::Unavailable,
            Recoverability::Retryable,
            "observe_runtime",
            "isolated audio plugin worker faulted during processing",
        )
        .with_context(
            ErrorContext::new(COMPONENT, "observe_runtime")
                .with_field("worker_faults", snapshot.worker_faults().to_string())
                .with_field("processed_blocks", snapshot.processed_blocks().to_string()),
        );
        record_failure(plugin, "process", &error, threshold);
        self.advance_revision()
    }

    /// Creates one revisioned project upsert from the latest retained checkpoint.
    pub fn checkpoint_command(
        &self,
        instance_id: AudioPluginInstanceId,
        identity: &AudioPluginIdentity,
    ) -> Result<ProjectExtensionCommand> {
        let plugin = self.plugin(identity, "checkpoint_command")?;
        let checkpoint = plugin.checkpoint.as_ref().ok_or_else(|| {
            plugin_error(
                ErrorCategory::NotFound,
                Recoverability::Retryable,
                "checkpoint_command",
                "audio plugin has no retained project checkpoint",
            )
        })?;
        Ok(ProjectExtensionCommand::upsert(
            audio_plugin_project_record(instance_id, checkpoint, plugin.lifecycle)?,
        ))
    }

    fn plugin(
        &self,
        identity: &AudioPluginIdentity,
        operation: &'static str,
    ) -> Result<&ManagedPlugin> {
        self.plugins
            .get(identity)
            .ok_or_else(|| missing_plugin(identity, operation))
    }

    fn plugin_mut(
        &mut self,
        identity: &AudioPluginIdentity,
        operation: &'static str,
    ) -> Result<&mut ManagedPlugin> {
        self.plugins
            .get_mut(identity)
            .ok_or_else(|| missing_plugin(identity, operation))
    }

    fn advance_revision(&mut self) -> Result<()> {
        self.revision = self.revision.checked_add(1).ok_or_else(|| {
            plugin_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::Terminal,
                "advance_revision",
                "audio plugin supervisor revision space is exhausted",
            )
        })?;
        Ok(())
    }
}

/// Creates the stable project record key for one exact audio plugin identity.
pub fn audio_plugin_project_key(instance_id: AudioPluginInstanceId) -> Result<ProjectExtensionKey> {
    let extension_id = project_extension_id()?;
    let record_id = ProjectExtensionRecordId::new(format!(
        "{PROJECT_RECORD_PREFIX}{:032x}",
        instance_id.raw()
    ))?;
    Ok(ProjectExtensionKey::new(extension_id, record_id))
}

/// Converts exact state plus authored enabled or disabled intent into the durable project envelope.
///
/// Fault and quarantine state remains operational supervisor evidence and is never persisted.
pub fn audio_plugin_project_record(
    instance_id: AudioPluginInstanceId,
    state: &AudioPluginState,
    lifecycle: AudioPluginLifecycle,
) -> Result<ProjectExtensionRecord> {
    let key = audio_plugin_project_key(instance_id)?;
    let project_lifecycle = match lifecycle {
        AudioPluginLifecycle::Disabled => ProjectExtensionLifecycle::Disabled,
        AudioPluginLifecycle::Ready
        | AudioPluginLifecycle::Faulted
        | AudioPluginLifecycle::Quarantined => ProjectExtensionLifecycle::Enabled,
    };
    ProjectExtensionRecord::new(
        key.extension_id().clone(),
        key.record_id().clone(),
        SemanticVersion::new(1, 0, 0),
        ProjectExtensionKind::plugin(),
        project_payload_schema()?,
        CapabilitySet::default(),
        CapabilitySet::default(),
        project_lifecycle,
        None,
        state.encode()?,
    )
}

/// Decodes and validates one audio plugin project record without changing unknown state.
pub fn audio_plugin_state_from_project_record(
    record: &ProjectExtensionRecord,
) -> Result<(AudioPluginInstanceId, AudioPluginState)> {
    if record.key().extension_id() != &project_extension_id()?
        || record.extension_version() != &SemanticVersion::new(1, 0, 0)
        || record.kind().as_str() != ProjectExtensionKind::plugin().as_str()
        || record.payload_schema() != &project_payload_schema()?
    {
        return Err(plugin_error(
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            "decode_project_record",
            "project extension record is not an audio plugin state envelope",
        ));
    }
    let record_id = record.key().record_id().as_str();
    let raw_instance = record_id
        .strip_prefix(PROJECT_RECORD_PREFIX)
        .filter(|value| value.len() == 32)
        .and_then(|value| u128::from_str_radix(value, 16).ok())
        .ok_or_else(|| {
            plugin_error(
                ErrorCategory::CorruptData,
                Recoverability::UserCorrectable,
                "decode_project_record",
                "audio plugin project record has an invalid instance identity",
            )
        })?;
    let instance_id = AudioPluginInstanceId::from_raw(raw_instance);
    let state = AudioPluginState::decode(record.payload())?;
    if record.key() != &audio_plugin_project_key(instance_id)? {
        return Err(plugin_error(
            ErrorCategory::CorruptData,
            Recoverability::UserCorrectable,
            "decode_project_record",
            "audio plugin project record key does not match its retained identity",
        ));
    }
    Ok((instance_id, state))
}

fn mark_healthy(plugin: &mut ManagedPlugin) {
    plugin.lifecycle = AudioPluginLifecycle::Ready;
    plugin.consecutive_failures = 0;
    plugin.last_failure = None;
}

fn record_failure(plugin: &mut ManagedPlugin, stage: &'static str, error: &Error, threshold: u32) {
    plugin.total_failures = plugin.total_failures.saturating_add(1);
    plugin.consecutive_failures = plugin.consecutive_failures.saturating_add(1);
    plugin.lifecycle = if plugin.consecutive_failures >= threshold {
        AudioPluginLifecycle::Quarantined
    } else {
        AudioPluginLifecycle::Faulted
    };
    plugin.last_failure = Some(AudioPluginFailure::from_error(
        stage,
        Some(plugin.source.clone()),
        error,
        plugin.total_failures,
        plugin.consecutive_failures,
    ));
}

fn project_extension_id() -> Result<ComponentId> {
    ComponentId::new(PROJECT_EXTENSION_ID).map_err(|source| {
        Error::with_source(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "built-in audio plugin extension identity is invalid",
            source,
        )
        .with_context(ErrorContext::new(COMPONENT, "project_schema"))
    })
}

fn project_payload_schema() -> Result<VersionIdentifier> {
    let component = ComponentId::new(PROJECT_PAYLOAD_ID).map_err(|source| {
        Error::with_source(
            ErrorCategory::Internal,
            Recoverability::Terminal,
            "built-in audio plugin payload identity is invalid",
            source,
        )
        .with_context(ErrorContext::new(COMPONENT, "project_schema"))
    })?;
    Ok(VersionIdentifier::new(
        component,
        SemanticVersion::new(1, 0, 0),
    ))
}

fn missing_plugin(identity: &AudioPluginIdentity, operation: &'static str) -> Error {
    plugin_error(
        ErrorCategory::NotFound,
        Recoverability::UserCorrectable,
        operation,
        "audio plugin supervisor does not contain the requested exact identity",
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("format", identity.format().code())
            .with_field("identifier", identity.identifier()),
    )
}

fn candidate_error(
    category: ErrorCategory,
    operation: &'static str,
    path: &Path,
    message: &'static str,
) -> Error {
    plugin_error(
        category,
        Recoverability::UserCorrectable,
        operation,
        message,
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation).with_field("path", path.display().to_string()),
    )
}

fn filesystem_error(operation: &'static str, path: &Path, source: std::io::Error) -> Error {
    let (category, recoverability) = match source.kind() {
        std::io::ErrorKind::NotFound => (ErrorCategory::NotFound, Recoverability::Degraded),
        std::io::ErrorKind::PermissionDenied => (
            ErrorCategory::PermissionDenied,
            Recoverability::UserCorrectable,
        ),
        _ => (ErrorCategory::Unavailable, Recoverability::Retryable),
    };
    Error::with_source(
        category,
        recoverability,
        "audio plugin filesystem discovery failed",
        source,
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation).with_field("path", path.display().to_string()),
    )
}

fn require_domain(domain: ExecutionDomain, operation: &'static str) -> Result<()> {
    domain.require_current().map_err(|mut error| {
        error.push_context(ErrorContext::new(COMPONENT, operation));
        error
    })
}

fn plugin_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
