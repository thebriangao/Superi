//! Engine-owned plugin discovery, isolated loading, and graph availability supervision.
//!
//! OpenFX bundle search is deterministic and failure-contained. Native code reaches the engine only
//! through a caller-owned [`OfxWorkerLauncher`] that must apply the target platform sandbox and
//! return the bounded worker-process contract validated by `superi-effects`. The supervisor owns
//! exact permission narrowing, lifecycle, recovery, quarantine, and one active graph schema view
//! shared by playback, rendering, and export.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fs;
use std::path::{Path, PathBuf};

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::settings::{CapabilityId, CapabilitySet};
use superi_effects::authoring::EffectNodeDefinition;
use superi_effects::ofx::{
    IsolatedOfxAdapter, OfxContext, OfxFailureRecord, OfxImageResource, OfxInstanceKey,
    OfxParameterSampler, OfxPluginHost, OfxPluginIdentity, OfxPluginLifecycle, OfxPluginStatus,
    OfxRenderReceipt, OfxRenderWindow, OfxTime,
};
use superi_graph::ids::NodeId;
use superi_graph::missing::{resolve_graph, GraphResolution};
use superi_graph::mutate::GraphSnapshot;
use superi_graph::node::{NodeRegistry, NodeRegistrySnapshot};
use superi_graph::value::GraphValue;

use crate::lifecycle::EngineWorkKind;

const COMPONENT: &str = "superi-engine.plugins";
const MAX_DISCOVERY_DEPTH: usize = 64;
const OFX_BUNDLE_SUFFIX: &str = ".ofx.bundle";

/// The engine response associated with one shared recovery classification.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum PluginFailureAction {
    /// Retry the failed isolated action or restart its worker.
    Retry,
    /// Preserve authored state and continue without the unavailable plugin result.
    ContinueDegraded,
    /// Ask the user to correct a bundle, permission, configuration, or project condition.
    CorrectConfiguration,
    /// Stop the unsafe operation in the current engine lifetime.
    Stop,
}

/// Cloneable plugin failure evidence retained across discovery and worker lifecycle changes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginFailure {
    stage: String,
    source: Option<String>,
    category: ErrorCategory,
    recoverability: Recoverability,
    message: String,
    contexts: Vec<ErrorContext>,
}

impl PluginFailure {
    fn from_error(stage: impl Into<String>, source: Option<String>, error: &Error) -> Self {
        Self {
            stage: stage.into(),
            source,
            category: error.category(),
            recoverability: error.recoverability(),
            message: error.message().to_owned(),
            contexts: error.contexts().to_vec(),
        }
    }

    fn from_ofx(identity: &OfxPluginIdentity, failure: &OfxFailureRecord) -> Self {
        Self {
            stage: failure.action().code().to_owned(),
            source: Some(identity.to_string()),
            category: ErrorCategory::from_code(failure.category())
                .unwrap_or(ErrorCategory::Internal),
            recoverability: Recoverability::from_code(failure.recoverability())
                .unwrap_or(Recoverability::Terminal),
            message: failure.message().to_owned(),
            contexts: Vec::new(),
        }
    }

    /// Returns the discovery, launch, validation, or worker action stage.
    #[must_use]
    pub fn stage(&self) -> &str {
        &self.stage
    }

    /// Returns the bundle path or exact plugin identity associated with the failure.
    #[must_use]
    pub fn source(&self) -> Option<&str> {
        self.source.as_deref()
    }

    /// Returns the stable shared failure category.
    #[must_use]
    pub const fn category(&self) -> ErrorCategory {
        self.category
    }

    /// Returns the exact shared recovery classification.
    #[must_use]
    pub const fn recoverability(&self) -> Recoverability {
        self.recoverability
    }

    /// Returns the concise diagnostic summary.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns structured context frames in failure-to-caller order.
    #[must_use]
    pub fn contexts(&self) -> &[ErrorContext] {
        &self.contexts
    }

    /// Maps the shared classification to the engine action without inspecting category text.
    #[must_use]
    pub const fn recommended_action(&self) -> PluginFailureAction {
        match self.recoverability {
            Recoverability::Retryable => PluginFailureAction::Retry,
            Recoverability::Degraded => PluginFailureAction::ContinueDegraded,
            Recoverability::UserCorrectable => PluginFailureAction::CorrectConfiguration,
            Recoverability::Terminal => PluginFailureAction::Stop,
            _ => PluginFailureAction::Stop,
        }
    }
}

/// One validated OpenFX package directory discovered without loading native code.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OfxPluginBundle {
    name: String,
    path: PathBuf,
    contents_path: PathBuf,
}

impl OfxPluginBundle {
    /// Validates one `NAME.ofx.bundle/Contents` package and resolves its stable local path.
    ///
    /// Architecture-specific binary selection and signature validation remain the platform
    /// launcher's responsibility so no native binary is mapped into the editor process.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let name = path
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| {
                bundle_error(&path, "non_utf8_name", "OpenFX bundle name must be UTF-8")
            })?
            .to_owned();
        if !is_bundle_name(&name) {
            return Err(bundle_error(
                &path,
                "invalid_suffix",
                "OpenFX bundle name must end in .ofx.bundle",
            ));
        }

        let metadata = fs::symlink_metadata(&path)
            .map_err(|source| filesystem_error("inspect_bundle", &path, source))?;
        if metadata.file_type().is_symlink() {
            return Err(Error::new(
                ErrorCategory::PermissionDenied,
                Recoverability::UserCorrectable,
                "OpenFX bundle path cannot be a symbolic link",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "inspect_bundle")
                    .with_field("path", path.display().to_string())
                    .with_field("reason", "symbolic_link"),
            ));
        }
        if !metadata.is_dir() {
            return Err(bundle_error(
                &path,
                "not_directory",
                "OpenFX bundle path must be a directory",
            ));
        }

        let contents_path = path.join("Contents");
        let contents_metadata = fs::symlink_metadata(&contents_path).map_err(|source| {
            if source.kind() == std::io::ErrorKind::NotFound {
                bundle_error(
                    &path,
                    "missing_contents",
                    "OpenFX bundle is missing its Contents directory",
                )
            } else {
                filesystem_error("inspect_bundle_contents", &contents_path, source)
            }
        })?;
        if contents_metadata.file_type().is_symlink() || !contents_metadata.is_dir() {
            return Err(bundle_error(
                &path,
                "invalid_contents",
                "OpenFX bundle Contents entry must be a real directory",
            ));
        }

        let path = fs::canonicalize(&path)
            .map_err(|source| filesystem_error("canonicalize_bundle", &path, source))?;
        let contents_path = path.join("Contents");
        Ok(Self {
            name,
            path,
            contents_path,
        })
    }

    /// Returns the canonical package directory name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the canonical local package path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Returns the validated package `Contents` path.
    #[must_use]
    pub fn contents_path(&self) -> &Path {
        &self.contents_path
    }
}

/// Deterministic bundle results plus contained filesystem and package failures.
#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct PluginDiscoveryReport {
    bundles: Vec<OfxPluginBundle>,
    failures: Vec<PluginFailure>,
}

impl PluginDiscoveryReport {
    /// Returns valid bundles in canonical path order.
    #[must_use]
    pub fn bundles(&self) -> &[OfxPluginBundle] {
        &self.bundles
    }

    /// Returns contained discovery failures in deterministic traversal order.
    #[must_use]
    pub fn failures(&self) -> &[PluginFailure] {
        &self.failures
    }
}

/// Returns OpenFX search roots from `OFX_PLUGIN_PATH` followed by the platform default.
///
/// macOS and Windows use the OpenFX-specified semicolon separator. Other Unix targets use a colon.
/// Missing roots remain visible when passed to [`discover_ofx_bundles`].
#[must_use]
pub fn standard_ofx_search_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let mut seen = BTreeSet::new();
    if let Some(value) = std::env::var_os("OFX_PLUGIN_PATH") {
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        let separator = ';';
        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        let separator = ':';
        for value in value.to_string_lossy().split(separator) {
            push_unique_root(&mut roots, &mut seen, PathBuf::from(value));
        }
    }

    #[cfg(target_os = "macos")]
    push_unique_root(&mut roots, &mut seen, PathBuf::from("/Library/OFX/Plugins"));
    #[cfg(all(unix, not(target_os = "macos")))]
    push_unique_root(&mut roots, &mut seen, PathBuf::from("/usr/OFX/Plugins"));
    #[cfg(target_os = "windows")]
    if let Some(program_files) = std::env::var_os("ProgramFiles") {
        push_unique_root(
            &mut roots,
            &mut seen,
            PathBuf::from(program_files).join("Common Files/OFX/Plugins"),
        );
    }
    roots
}

/// Recursively discovers validated OpenFX bundles without loading plugin code.
///
/// Any bundle or intermediate directory beginning with `@` is skipped as required by OpenFX 1.5.1.
/// Filesystem and malformed-package failures are retained while unrelated roots and bundles
/// continue. Symbolic-link directories are never traversed.
#[must_use]
pub fn discover_ofx_bundles<I, P>(roots: I) -> PluginDiscoveryReport
where
    I: IntoIterator<Item = P>,
    P: Into<PathBuf>,
{
    let mut bundles = BTreeMap::new();
    let mut failures = Vec::new();
    let mut queue = roots
        .into_iter()
        .map(|root| (root.into(), 0_usize))
        .collect::<VecDeque<_>>();

    while let Some((path, depth)) = queue.pop_front() {
        if depth > 0 && path_name_starts_with_at(&path) {
            continue;
        }
        let metadata = match fs::symlink_metadata(&path) {
            Ok(metadata) => metadata,
            Err(source) => {
                let error = filesystem_error("discover", &path, source);
                failures.push(PluginFailure::from_error(
                    "discover",
                    Some(path.display().to_string()),
                    &error,
                ));
                continue;
            }
        };
        if metadata.file_type().is_symlink() {
            if path
                .file_name()
                .and_then(|name| name.to_str())
                .is_some_and(is_bundle_name)
            {
                let error = Error::new(
                    ErrorCategory::PermissionDenied,
                    Recoverability::UserCorrectable,
                    "OpenFX discovery does not follow symbolic-link bundles",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "discover")
                        .with_field("path", path.display().to_string()),
                );
                failures.push(PluginFailure::from_error(
                    "discover",
                    Some(path.display().to_string()),
                    &error,
                ));
            }
            continue;
        }
        if !metadata.is_dir() {
            if depth == 0 {
                let error = bundle_error(
                    &path,
                    "search_root_not_directory",
                    "OpenFX search root must be a directory",
                );
                failures.push(PluginFailure::from_error(
                    "discover",
                    Some(path.display().to_string()),
                    &error,
                ));
            }
            continue;
        }

        if path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(is_bundle_name)
        {
            match OfxPluginBundle::open(path.clone()) {
                Ok(bundle) => {
                    bundles.entry(bundle.path.clone()).or_insert(bundle);
                }
                Err(error) => failures.push(PluginFailure::from_error(
                    "validate_bundle",
                    Some(path.display().to_string()),
                    &error,
                )),
            }
            continue;
        }

        if depth >= MAX_DISCOVERY_DEPTH {
            let error = Error::new(
                ErrorCategory::ResourceExhausted,
                Recoverability::Degraded,
                "OpenFX discovery reached its bounded recursion depth",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "discover")
                    .with_field("path", path.display().to_string())
                    .with_field("max_depth", MAX_DISCOVERY_DEPTH.to_string()),
            );
            failures.push(PluginFailure::from_error(
                "discover",
                Some(path.display().to_string()),
                &error,
            ));
            continue;
        }

        let entries = match fs::read_dir(&path) {
            Ok(entries) => entries,
            Err(source) => {
                let error = filesystem_error("read_search_directory", &path, source);
                failures.push(PluginFailure::from_error(
                    "discover",
                    Some(path.display().to_string()),
                    &error,
                ));
                continue;
            }
        };
        let mut children = Vec::new();
        for entry in entries {
            match entry {
                Ok(entry) => children.push(entry.path()),
                Err(source) => {
                    let error = filesystem_error("read_search_entry", &path, source);
                    failures.push(PluginFailure::from_error(
                        "discover",
                        Some(path.display().to_string()),
                        &error,
                    ));
                }
            }
        }
        children.sort();
        queue.extend(children.into_iter().map(|child| (child, depth + 1)));
    }

    PluginDiscoveryReport {
        bundles: bundles.into_values().collect(),
        failures,
    }
}

/// Platform-owned construction of one sandboxed, bounded OpenFX worker adapter.
///
/// Implementations must select and validate the architecture-specific binary inside the supplied
/// bundle, scan it before activation, apply the platform containment boundary, deny filesystem,
/// network, process, and device access by default, and expose only versioned bounded IPC. macOS
/// implementations should use an XPC privilege boundary, Windows implementations should use
/// AppContainer and job controls, and Linux implementations need namespace and policy isolation in
/// addition to syscall filtering. The returned adapter is rejected by `OfxPluginHost` unless it
/// reports an out-of-process, bounded, deadline-enforced, restartable contract.
pub trait OfxWorkerLauncher {
    /// Starts or connects to the isolated scan worker for one validated bundle.
    fn launch(&mut self, bundle: &OfxPluginBundle) -> Result<Box<dyn IsolatedOfxAdapter>>;
}

impl<F> OfxWorkerLauncher for F
where
    F: FnMut(&OfxPluginBundle) -> Result<Box<dyn IsolatedOfxAdapter>>,
{
    fn launch(&mut self, bundle: &OfxPluginBundle) -> Result<Box<dyn IsolatedOfxAdapter>> {
        self(bundle)
    }
}

type DynamicOfxHost = OfxPluginHost<Box<dyn IsolatedOfxAdapter>>;

#[derive(Debug)]
struct ManagedPlugin {
    bundle: OfxPluginBundle,
    host: DynamicOfxHost,
}

/// Immutable user-inspectable state for one accepted plugin.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PluginStatusSnapshot {
    bundle: OfxPluginBundle,
    identity: OfxPluginIdentity,
    status: OfxPluginStatus,
}

impl PluginStatusSnapshot {
    /// Returns the local package that supplied this plugin.
    #[must_use]
    pub const fn bundle(&self) -> &OfxPluginBundle {
        &self.bundle
    }

    /// Returns the exact scanned plugin identity and version.
    #[must_use]
    pub const fn identity(&self) -> &OfxPluginIdentity {
        &self.identity
    }

    /// Returns the current worker lifecycle.
    #[must_use]
    pub const fn lifecycle(&self) -> OfxPluginLifecycle {
        self.status.lifecycle()
    }

    /// Returns all adapter failures observed for this accepted plugin.
    #[must_use]
    pub const fn total_failures(&self) -> u64 {
        self.status.total_failures()
    }

    /// Returns failures since the last healthy disable or quarantine acknowledgement.
    #[must_use]
    pub const fn consecutive_failures(&self) -> u32 {
        self.status.consecutive_failures()
    }

    /// Returns the complete effects-owned lifecycle snapshot.
    #[must_use]
    pub const fn ofx_status(&self) -> &OfxPluginStatus {
        &self.status
    }
}

/// One engine-owned plugin discovery, lifecycle, and availability coordinator.
#[derive(Debug)]
pub struct PluginSupervisor {
    plugins: BTreeMap<String, ManagedPlugin>,
    retained_failures: Vec<PluginFailure>,
    discovered_registry: NodeRegistrySnapshot,
    active_registry: NodeRegistrySnapshot,
    revision: u64,
}

impl PluginSupervisor {
    /// Launches, scans, validates, and indexes every discovered bundle while containing failures.
    ///
    /// One launch, descriptor, identity, or schema failure is retained and does not abort unrelated
    /// bundles. A zero quarantine threshold is a configuration error because no worker could enter
    /// a usable lifecycle.
    pub fn scan(
        discovery: PluginDiscoveryReport,
        launcher: &mut impl OfxWorkerLauncher,
        quarantine_threshold: u32,
    ) -> Result<Self> {
        if quarantine_threshold == 0 {
            return Err(Error::new(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "plugin quarantine threshold must be positive",
            )
            .with_context(ErrorContext::new(COMPONENT, "scan")));
        }

        let PluginDiscoveryReport {
            bundles,
            mut failures,
        } = discovery;
        let mut plugins = BTreeMap::new();
        let mut discovered_registry = NodeRegistry::new();

        for bundle in bundles {
            let source = bundle.path().display().to_string();
            let adapter = match launcher.launch(&bundle) {
                Ok(adapter) => adapter,
                Err(error) => {
                    failures.push(PluginFailure::from_error("launch", Some(source), &error));
                    continue;
                }
            };
            let host = match OfxPluginHost::scan(adapter, quarantine_threshold) {
                Ok(host) => host,
                Err(error) => {
                    failures.push(PluginFailure::from_error("scan", Some(source), &error));
                    continue;
                }
            };
            let identity = host.plugin().identity().to_string();
            if plugins.contains_key(&identity) {
                let error = Error::new(
                    ErrorCategory::Conflict,
                    Recoverability::UserCorrectable,
                    "multiple OpenFX bundles expose the same exact plugin identity",
                )
                .with_context(
                    ErrorContext::new(COMPONENT, "scan")
                        .with_field("plugin", &identity)
                        .with_field("bundle", &source),
                );
                failures.push(PluginFailure::from_error(
                    "validate_identity",
                    Some(source),
                    &error,
                ));
                continue;
            }

            let catalog = match host.discovered_catalog::<()>() {
                Ok(catalog) => catalog,
                Err(error) => {
                    failures.push(PluginFailure::from_error(
                        "project_schema",
                        Some(source),
                        &error,
                    ));
                    continue;
                }
            };
            let catalog = catalog.snapshot();
            let schemas = catalog.node_schemas().iter().cloned().collect::<Vec<_>>();
            let mut candidate_registry = discovered_registry.clone();
            if let Err(error) = candidate_registry.register_batch(schemas) {
                failures.push(PluginFailure::from_error(
                    "validate_schema",
                    Some(source),
                    &error,
                ));
                continue;
            }
            discovered_registry = candidate_registry;
            plugins.insert(identity, ManagedPlugin { bundle, host });
        }

        Ok(Self {
            plugins,
            retained_failures: failures,
            discovered_registry: discovered_registry.snapshot(),
            active_registry: NodeRegistry::new().snapshot(),
            revision: 1,
        })
    }

    /// Discovers explicit roots and immediately runs the contained scan pipeline.
    pub fn discover_and_scan<I, P>(
        roots: I,
        launcher: &mut impl OfxWorkerLauncher,
        quarantine_threshold: u32,
    ) -> Result<Self>
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        Self::scan(discover_ofx_bundles(roots), launcher, quarantine_threshold)
    }

    /// Returns the current supervisor state revision.
    #[must_use]
    pub const fn revision(&self) -> u64 {
        self.revision
    }

    /// Returns the number of accepted exact plugin identities.
    #[must_use]
    pub fn plugin_count(&self) -> usize {
        self.plugins.len()
    }

    /// Returns every scanned schema, including disabled and unavailable plugins.
    #[must_use]
    pub const fn discovered_registry(&self) -> &NodeRegistrySnapshot {
        &self.discovered_registry
    }

    /// Returns only schemas whose plugin workers are currently ready.
    #[must_use]
    pub const fn active_registry(&self) -> &NodeRegistrySnapshot {
        &self.active_registry
    }

    /// Returns retained discovery failures plus each accepted plugin's latest worker failure.
    #[must_use]
    pub fn failures(&self) -> Vec<PluginFailure> {
        let mut failures = self.retained_failures.clone();
        failures.extend(self.plugins.values().filter_map(|plugin| {
            let status = plugin.host.status();
            status
                .last_failure()
                .map(|failure| PluginFailure::from_ofx(plugin.host.plugin().identity(), failure))
        }));
        failures
    }

    /// Returns one accepted plugin's immutable lifecycle snapshot.
    #[must_use]
    pub fn plugin_status(&self, identity: &OfxPluginIdentity) -> Option<PluginStatusSnapshot> {
        let plugin = self.plugins.get(&identity.to_string())?;
        Some(PluginStatusSnapshot {
            bundle: plugin.bundle.clone(),
            identity: plugin.host.plugin().identity().clone(),
            status: plugin.host.status(),
        })
    }

    /// Builds one graph-native definition from a scanned plugin context.
    pub fn definition<T>(
        &self,
        identity: &OfxPluginIdentity,
        context: OfxContext,
    ) -> Result<EffectNodeDefinition<GraphValue<T>>> {
        self.plugin(identity, "definition")?
            .host
            .definition(context)
            .map_err(|mut error| {
                error.push_context(plugin_context("definition", identity));
                error
            })
    }

    /// Grants exactly the plugin's requested permissions after proving caller authorization.
    ///
    /// Extra capabilities in `authorized_permissions` are deliberately not forwarded to the worker.
    pub fn enable(
        &mut self,
        identity: &OfxPluginIdentity,
        authorized_permissions: &CapabilitySet,
    ) -> Result<()> {
        let requested = self
            .plugin(identity, "enable")?
            .host
            .plugin()
            .requested_permissions()
            .clone();
        if !authorized_permissions.contains_all(&requested) {
            let missing = requested
                .iter()
                .filter(|capability| !authorized_permissions.contains(capability))
                .map(CapabilityId::as_str)
                .collect::<Vec<_>>()
                .join(",");
            return Err(Error::new(
                ErrorCategory::PermissionDenied,
                Recoverability::UserCorrectable,
                "plugin activation requires explicit authorization for every requested capability",
            )
            .with_context(
                plugin_context("enable", identity).with_field("missing_permissions", missing),
            ));
        }
        self.mutate_plugin(identity, "enable", move |host| host.enable(&requested))
    }

    /// Unloads one ready plugin after all of its instances are destroyed.
    pub fn disable(&mut self, identity: &OfxPluginIdentity) -> Result<()> {
        self.mutate_plugin(identity, "disable", OfxPluginHost::disable)
    }

    /// Restarts one faulted worker while retaining its consecutive failure count.
    pub fn recover(&mut self, identity: &OfxPluginIdentity) -> Result<()> {
        self.mutate_plugin(identity, "recover", OfxPluginHost::recover)
    }

    /// Acknowledges quarantine, restarts the worker, and clears its consecutive failure count.
    pub fn clear_quarantine(&mut self, identity: &OfxPluginIdentity) -> Result<()> {
        self.mutate_plugin(
            identity,
            "clear_quarantine",
            OfxPluginHost::clear_quarantine,
        )
    }

    /// Creates one graph-owned plugin instance through the supervised adapter.
    #[allow(clippy::too_many_arguments)]
    pub fn create_instance<T: Clone>(
        &mut self,
        identity: &OfxPluginIdentity,
        context: OfxContext,
        snapshot: &GraphSnapshot<GraphValue<T>>,
        node_id: NodeId,
        time: OfxTime,
        sampler: &mut impl OfxParameterSampler<T>,
    ) -> Result<()> {
        self.mutate_plugin(identity, "create_instance", |host| {
            host.create_instance(context, snapshot, node_id, time, sampler)
        })
    }

    /// Renders one bounded plugin request and immediately republishes availability after failure.
    #[allow(clippy::too_many_arguments)]
    pub fn render<T: Clone>(
        &mut self,
        identity: &OfxPluginIdentity,
        key: OfxInstanceKey,
        snapshot: &GraphSnapshot<GraphValue<T>>,
        time: OfxTime,
        sampler: &mut impl OfxParameterSampler<T>,
        window: OfxRenderWindow,
        images: impl IntoIterator<Item = OfxImageResource>,
    ) -> Result<OfxRenderReceipt> {
        self.mutate_plugin(identity, "render", |host| {
            host.render(key, snapshot, time, sampler, window, images)
        })
    }

    /// Destroys one ready graph-owned plugin instance.
    pub fn destroy_instance(
        &mut self,
        identity: &OfxPluginIdentity,
        key: OfxInstanceKey,
    ) -> Result<()> {
        self.mutate_plugin(identity, "destroy_instance", |host| {
            host.destroy_instance(key)
        })
    }

    /// Resolves one exact graph against the same active plugin state used by every engine workflow.
    #[must_use]
    pub fn resolve_work<T: Clone>(
        &self,
        work: EngineWorkKind,
        graph: &GraphSnapshot<GraphValue<T>>,
    ) -> PluginWorkResolution<T> {
        PluginWorkResolution {
            work,
            supervisor_revision: self.revision,
            resolution: resolve_graph(graph, &self.active_registry),
            failures: self.failures(),
        }
    }

    fn plugin(
        &self,
        identity: &OfxPluginIdentity,
        operation: &'static str,
    ) -> Result<&ManagedPlugin> {
        self.plugins.get(&identity.to_string()).ok_or_else(|| {
            Error::new(
                ErrorCategory::NotFound,
                Recoverability::UserCorrectable,
                "plugin supervisor does not contain the requested exact identity",
            )
            .with_context(plugin_context(operation, identity))
        })
    }

    fn mutate_plugin<R>(
        &mut self,
        identity: &OfxPluginIdentity,
        operation: &'static str,
        action: impl FnOnce(&mut DynamicOfxHost) -> Result<R>,
    ) -> Result<R> {
        let next_revision = self.revision.checked_add(1).ok_or_else(|| {
            Error::new(
                ErrorCategory::ResourceExhausted,
                Recoverability::Terminal,
                "plugin supervisor revision space is exhausted",
            )
            .with_context(plugin_context(operation, identity))
        })?;
        let result = {
            let plugin = self.plugins.get_mut(&identity.to_string()).ok_or_else(|| {
                Error::new(
                    ErrorCategory::NotFound,
                    Recoverability::UserCorrectable,
                    "plugin supervisor does not contain the requested exact identity",
                )
                .with_context(plugin_context(operation, identity))
            })?;
            action(&mut plugin.host).map_err(|mut error| {
                error.push_context(
                    plugin_context(operation, identity)
                        .with_field("bundle", plugin.bundle.path().display().to_string()),
                );
                error
            })
        };

        let registry_result = self.build_active_registry();
        match (result, registry_result) {
            (Ok(value), Ok(registry)) => {
                self.active_registry = registry;
                self.revision = next_revision;
                Ok(value)
            }
            (Err(error), Ok(registry)) => {
                self.active_registry = registry;
                self.revision = next_revision;
                Err(error)
            }
            (Ok(_), Err(error)) => Err(error),
            (Err(mut action_error), Err(registry_error)) => {
                action_error.push_context(
                    plugin_context("rebuild_active_registry", identity)
                        .with_field("registry_error", registry_error.to_string()),
                );
                Err(action_error)
            }
        }
    }

    fn build_active_registry(&self) -> Result<NodeRegistrySnapshot> {
        let mut registry = NodeRegistry::new();
        for plugin in self.plugins.values() {
            if plugin.host.lifecycle() != OfxPluginLifecycle::Ready {
                continue;
            }
            let catalog = plugin.host.active_catalog::<()>().map_err(|mut error| {
                error.push_context(
                    ErrorContext::new(COMPONENT, "rebuild_active_registry")
                        .with_field("plugin", plugin.host.plugin().identity().to_string()),
                );
                error
            })?;
            let catalog = catalog.snapshot();
            registry.register_batch(catalog.node_schemas().iter().cloned())?;
        }
        Ok(registry.snapshot())
    }
}

/// One immutable graph availability result for a canonical engine work kind.
#[derive(Clone, Debug)]
pub struct PluginWorkResolution<T> {
    work: EngineWorkKind,
    supervisor_revision: u64,
    resolution: GraphResolution<GraphValue<T>>,
    failures: Vec<PluginFailure>,
}

impl<T> PluginWorkResolution<T> {
    /// Returns the canonical playback, rendering, or export work kind.
    #[must_use]
    pub const fn work(&self) -> EngineWorkKind {
        self.work
    }

    /// Returns the exact supervisor state revision used for graph resolution.
    #[must_use]
    pub const fn supervisor_revision(&self) -> u64 {
        self.supervisor_revision
    }

    /// Returns the graph-owned resolution without rewriting authored state.
    #[must_use]
    pub const fn resolution(&self) -> &GraphResolution<GraphValue<T>> {
        &self.resolution
    }

    /// Returns the number of graph nodes unavailable in this exact plugin state.
    #[must_use]
    pub fn missing_node_count(&self) -> usize {
        self.resolution.missing_node_count()
    }

    /// Returns discovery and worker failure evidence available at resolution time.
    #[must_use]
    pub fn failures(&self) -> &[PluginFailure] {
        &self.failures
    }

    /// Returns the authored graph only when every exact plugin schema is active.
    pub fn require_evaluable(&self) -> Result<&GraphSnapshot<GraphValue<T>>> {
        self.resolution.require_evaluable().map_err(|mut error| {
            error.push_context(
                ErrorContext::new(COMPONENT, "require_work_graph")
                    .with_field("work", self.work.code())
                    .with_field("supervisor_revision", self.supervisor_revision.to_string()),
            );
            error
        })
    }
}

fn push_unique_root(roots: &mut Vec<PathBuf>, seen: &mut BTreeSet<PathBuf>, root: PathBuf) {
    if !root.as_os_str().is_empty() && seen.insert(root.clone()) {
        roots.push(root);
    }
}

fn path_name_starts_with_at(path: &Path) -> bool {
    path.file_name()
        .map(|name| name.to_string_lossy().starts_with('@'))
        .unwrap_or(false)
}

fn is_bundle_name(name: &str) -> bool {
    name.len() > OFX_BUNDLE_SUFFIX.len() && name.ends_with(OFX_BUNDLE_SUFFIX)
}

fn bundle_error(path: &Path, reason: &'static str, message: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(
        ErrorContext::new(COMPONENT, "validate_bundle")
            .with_field("path", path.display().to_string())
            .with_field("reason", reason),
    )
}

fn filesystem_error(operation: &'static str, path: &Path, source: std::io::Error) -> Error {
    let (category, recoverability) = match source.kind() {
        std::io::ErrorKind::NotFound => (ErrorCategory::NotFound, Recoverability::Degraded),
        std::io::ErrorKind::PermissionDenied => (
            ErrorCategory::PermissionDenied,
            Recoverability::UserCorrectable,
        ),
        std::io::ErrorKind::InvalidData => {
            (ErrorCategory::CorruptData, Recoverability::UserCorrectable)
        }
        _ => (ErrorCategory::Unavailable, Recoverability::Retryable),
    };
    Error::with_source(
        category,
        recoverability,
        "OpenFX filesystem discovery failed",
        source,
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation).with_field("path", path.display().to_string()),
    )
}

fn plugin_context(operation: &'static str, identity: &OfxPluginIdentity) -> ErrorContext {
    ErrorContext::new(COMPONENT, operation).with_field("plugin", identity.to_string())
}
