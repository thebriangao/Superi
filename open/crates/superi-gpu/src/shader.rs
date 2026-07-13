//! Canonical WGSL compilation, reflection, validation, and bounded caching.

use std::borrow::Cow;
use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::num::{NonZeroU32, NonZeroUsize};
use std::sync::{Arc, Mutex, MutexGuard};

use sha2::{Digest, Sha256};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

use crate::resource::{GpuResourceId, GpuResourceKind, GpuResources, ResourceLease};

const COMPONENT: &str = "superi-gpu.shader";

/// Maximum accepted WGSL source size for one module.
pub const MAX_WGSL_SOURCE_BYTES: usize = 4 * 1024 * 1024;

/// A WGSL shader-module creation descriptor.
#[derive(Clone, Copy, Debug)]
pub struct GpuShaderModuleDescriptor<'a> {
    /// Debug label forwarded to wgpu and included in cache identity.
    pub label: Option<&'a str>,
    /// Canonical WGSL source text.
    pub source: &'a str,
}

/// One programmable shader stage.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub enum ShaderStage {
    /// Vertex processing.
    Vertex,
    /// Fragment processing.
    Fragment,
    /// Compute processing.
    Compute,
}

impl ShaderStage {
    /// Returns the corresponding wgpu visibility bit.
    #[must_use]
    pub const fn visibility(self) -> wgpu::ShaderStages {
        match self {
            Self::Vertex => wgpu::ShaderStages::VERTEX,
            Self::Fragment => wgpu::ShaderStages::FRAGMENT,
            Self::Compute => wgpu::ShaderStages::COMPUTE,
        }
    }
}

impl From<wgpu::naga::ShaderStage> for ShaderStage {
    fn from(value: wgpu::naga::ShaderStage) -> Self {
        match value {
            wgpu::naga::ShaderStage::Vertex => Self::Vertex,
            wgpu::naga::ShaderStage::Fragment => Self::Fragment,
            wgpu::naga::ShaderStage::Compute => Self::Compute,
        }
    }
}

/// One reflected entry point.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShaderEntryPoint {
    name: String,
    stage: ShaderStage,
    workgroup_size: Option<[u32; 3]>,
}

impl ShaderEntryPoint {
    /// Returns the source-level entry-point name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the programmable stage.
    #[must_use]
    pub const fn stage(&self) -> ShaderStage {
        self.stage
    }

    /// Returns the compute workgroup size, or None for graphics stages.
    #[must_use]
    pub const fn workgroup_size(&self) -> Option<[u32; 3]> {
        self.workgroup_size
    }
}

/// Access declared for a reflected storage texture.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ShaderStorageAccess {
    /// Read access only.
    ReadOnly,
    /// Write access only.
    WriteOnly,
    /// Read and write access.
    ReadWrite,
    /// No load or store access was declared.
    None,
}

/// One reflected shader resource class.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ShaderBindingKind {
    /// Uniform buffer.
    UniformBuffer,
    /// Storage buffer with explicit mutability.
    StorageBuffer {
        /// True when shaders cannot write the buffer.
        read_only: bool,
    },
    /// Regular or comparison sampler.
    Sampler {
        /// True for comparison samplers.
        comparison: bool,
    },
    /// Sampled or depth texture.
    SampledTexture {
        /// Texture dimensionality.
        dimension: wgpu::naga::ImageDimension,
        /// True for an array texture.
        arrayed: bool,
        /// Scalar kind returned by sampling, or None for depth textures.
        sample_kind: Option<wgpu::naga::ScalarKind>,
        /// True for a multisampled texture.
        multisampled: bool,
    },
    /// Storage texture.
    StorageTexture {
        /// Texture dimensionality.
        dimension: wgpu::naga::ImageDimension,
        /// True for an array texture.
        arrayed: bool,
        /// Declared texel format.
        format: wgpu::naga::StorageFormat,
        /// Declared read and write access.
        access: ShaderStorageAccess,
    },
    /// Ray-tracing acceleration structure.
    AccelerationStructure,
    /// A future Naga resource class not known to this version.
    Other,
}

/// One reflected bind-group resource.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShaderBinding {
    group: u32,
    binding: u32,
    name: Option<String>,
    visibility: wgpu::ShaderStages,
    kind: ShaderBindingKind,
    array_count: Option<NonZeroU32>,
}

impl ShaderBinding {
    /// Returns the bind-group index.
    #[must_use]
    pub const fn group(&self) -> u32 {
        self.group
    }

    /// Returns the binding number within the group.
    #[must_use]
    pub const fn binding(&self) -> u32 {
        self.binding
    }

    /// Returns the source-level variable name, when present.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns every entry-point stage that statically uses this resource.
    #[must_use]
    pub const fn visibility(&self) -> wgpu::ShaderStages {
        self.visibility
    }

    /// Returns the reflected resource class.
    #[must_use]
    pub const fn kind(&self) -> ShaderBindingKind {
        self.kind
    }

    /// Returns the fixed binding-array length, when this is a binding array.
    #[must_use]
    pub const fn array_count(&self) -> Option<NonZeroU32> {
        self.array_count
    }
}

/// One reflected pipeline-overridable constant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShaderOverride {
    name: Option<String>,
    id: Option<u16>,
    has_default: bool,
}

impl ShaderOverride {
    /// Returns the source-level name, when declared.
    #[must_use]
    pub fn name(&self) -> Option<&str> {
        self.name.as_deref()
    }

    /// Returns the explicit numeric identifier, when declared.
    #[must_use]
    pub const fn id(&self) -> Option<u16> {
        self.id
    }

    /// Returns whether the override has a default value.
    #[must_use]
    pub const fn has_default(&self) -> bool {
        self.has_default
    }
}

/// Deterministic, source-derived shader interface metadata.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShaderReflection {
    entry_points: Vec<ShaderEntryPoint>,
    bindings: Vec<ShaderBinding>,
    overrides: Vec<ShaderOverride>,
}

impl ShaderReflection {
    /// Returns entry points in source order.
    #[must_use]
    pub fn entry_points(&self) -> &[ShaderEntryPoint] {
        &self.entry_points
    }

    /// Finds an entry point by exact name and stage.
    #[must_use]
    pub fn entry_point(&self, name: &str, stage: ShaderStage) -> Option<&ShaderEntryPoint> {
        self.entry_points
            .iter()
            .find(|entry| entry.name == name && entry.stage == stage)
    }

    /// Returns bindings sorted by group and binding number.
    #[must_use]
    pub fn bindings(&self) -> &[ShaderBinding] {
        &self.bindings
    }

    /// Finds one reflected binding.
    #[must_use]
    pub fn binding(&self, group: u32, binding: u32) -> Option<&ShaderBinding> {
        self.bindings
            .iter()
            .find(|item| item.group == group && item.binding == binding)
    }

    /// Returns pipeline-overridable constants in source order.
    #[must_use]
    pub fn overrides(&self) -> &[ShaderOverride] {
        &self.overrides
    }

    /// Finds a named pipeline-overridable constant.
    #[must_use]
    pub fn override_named(&self, name: &str) -> Option<&ShaderOverride> {
        self.overrides
            .iter()
            .find(|item| item.name.as_deref() == Some(name))
    }
}

/// Severity of one backend shader compilation diagnostic.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub enum ShaderDiagnosticSeverity {
    /// Compilation failure.
    Error,
    /// Compilation warning.
    Warning,
    /// Informational compiler message.
    Info,
}

/// UTF-8 byte location in WGSL source.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct ShaderSourceLocation {
    line: u32,
    column: u32,
    offset: u32,
    length: u32,
}

impl ShaderSourceLocation {
    /// Returns the 1-based line number.
    #[must_use]
    pub const fn line(self) -> u32 {
        self.line
    }

    /// Returns the 1-based UTF-8 byte column.
    #[must_use]
    pub const fn column(self) -> u32 {
        self.column
    }

    /// Returns the 0-based UTF-8 byte offset.
    #[must_use]
    pub const fn offset(self) -> u32 {
        self.offset
    }

    /// Returns the UTF-8 byte length.
    #[must_use]
    pub const fn length(self) -> u32 {
        self.length
    }
}

impl From<wgpu::SourceLocation> for ShaderSourceLocation {
    fn from(value: wgpu::SourceLocation) -> Self {
        Self {
            line: value.line_number,
            column: value.line_position,
            offset: value.offset,
            length: value.length,
        }
    }
}

impl From<wgpu::naga::SourceLocation> for ShaderSourceLocation {
    fn from(value: wgpu::naga::SourceLocation) -> Self {
        Self {
            line: value.line_number,
            column: value.line_position,
            offset: value.offset,
            length: value.length,
        }
    }
}

/// One backend compilation message retained without shader source text.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ShaderDiagnostic {
    severity: ShaderDiagnosticSeverity,
    message: String,
    location: Option<ShaderSourceLocation>,
}

impl ShaderDiagnostic {
    /// Returns diagnostic severity.
    #[must_use]
    pub const fn severity(&self) -> ShaderDiagnosticSeverity {
        self.severity
    }

    /// Returns the backend message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns the source location when the backend supplied one.
    #[must_use]
    pub const fn location(&self) -> Option<ShaderSourceLocation> {
        self.location
    }
}

/// Immutable metadata for a compiled shader module.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct GpuShaderModuleInfo {
    label: Option<String>,
    source_digest: [u8; 32],
    source_bytes: usize,
    reflection: ShaderReflection,
    diagnostics: Vec<ShaderDiagnostic>,
}

impl GpuShaderModuleInfo {
    /// Returns the debug label, when present.
    #[must_use]
    pub fn label(&self) -> Option<&str> {
        self.label.as_deref()
    }

    /// Returns the SHA-256 digest of the exact WGSL source bytes.
    #[must_use]
    pub const fn source_digest(&self) -> [u8; 32] {
        self.source_digest
    }

    /// Returns the exact WGSL source size in bytes.
    #[must_use]
    pub const fn source_bytes(&self) -> usize {
        self.source_bytes
    }

    /// Returns deterministic source reflection.
    #[must_use]
    pub const fn reflection(&self) -> &ShaderReflection {
        &self.reflection
    }

    /// Returns backend warnings and informational messages.
    #[must_use]
    pub fn diagnostics(&self) -> &[ShaderDiagnostic] {
        &self.diagnostics
    }
}

#[derive(Debug)]
struct GpuShaderModuleInner {
    lease: ResourceLease,
    raw: wgpu::ShaderModule,
    info: GpuShaderModuleInfo,
}

/// A cloneable validated shader module owned by one GPU device lifetime.
#[derive(Clone, Debug)]
pub struct GpuShaderModule(Arc<GpuShaderModuleInner>);

impl GpuShaderModule {
    /// Returns this module's process-local diagnostic identifier.
    #[must_use]
    pub fn id(&self) -> GpuResourceId {
        self.0.lease.id()
    }

    /// Returns immutable module metadata.
    #[must_use]
    pub fn info(&self) -> &GpuShaderModuleInfo {
        &self.0.info
    }

    /// Returns deterministic source reflection.
    #[must_use]
    pub fn reflection(&self) -> &ShaderReflection {
        self.0.info.reflection()
    }

    /// Returns backend warnings and informational messages.
    #[must_use]
    pub fn diagnostics(&self) -> &[ShaderDiagnostic] {
        self.0.info.diagnostics()
    }

    pub(crate) fn raw(&self) -> &wgpu::ShaderModule {
        &self.0.raw
    }

    pub(crate) fn lease(&self) -> &ResourceLease {
        &self.0.lease
    }
}

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct ShaderCacheKey {
    label: Option<Arc<str>>,
    source: Arc<str>,
}

#[derive(Debug)]
struct ShaderCacheEntry {
    module: GpuShaderModule,
    last_used: u64,
}

#[derive(Debug, Default)]
struct ShaderCacheState {
    entries: BTreeMap<ShaderCacheKey, ShaderCacheEntry>,
    clock: u64,
    hits: u64,
    misses: u64,
    evictions: u64,
}

impl ShaderCacheState {
    fn tick(&mut self) -> u64 {
        self.clock = self.clock.saturating_add(1);
        self.clock
    }
}

/// Observable shader-cache counters.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ShaderCacheStats {
    entries: usize,
    hits: u64,
    misses: u64,
    evictions: u64,
}

impl ShaderCacheStats {
    /// Returns the current number of strongly retained cache entries.
    #[must_use]
    pub const fn entries(self) -> usize {
        self.entries
    }

    /// Returns the number of exact cache hits.
    #[must_use]
    pub const fn hits(self) -> u64 {
        self.hits
    }

    /// Returns the number of cache misses, including failed compilations.
    #[must_use]
    pub const fn misses(self) -> u64 {
        self.misses
    }

    /// Returns the number of entries removed for capacity.
    #[must_use]
    pub const fn evictions(self) -> u64 {
        self.evictions
    }
}

/// A bounded per-device cache of validated WGSL shader modules.
#[derive(Debug)]
pub struct ShaderCache<'device> {
    resources: GpuResources<'device>,
    capacity: NonZeroUsize,
    state: Mutex<ShaderCacheState>,
}

impl<'device> ShaderCache<'device> {
    /// Creates a cache scoped to one managed device lifetime.
    #[must_use]
    pub fn new(resources: &GpuResources<'device>, capacity: NonZeroUsize) -> Self {
        Self {
            resources: resources.clone(),
            capacity,
            state: Mutex::new(ShaderCacheState::default()),
        }
    }

    /// Compiles, reflects, validates, and caches one exact WGSL descriptor.
    pub async fn compile_wgsl(
        &self,
        descriptor: GpuShaderModuleDescriptor<'_>,
    ) -> Result<GpuShaderModule> {
        validate_source_bounds(descriptor)?;
        let key = ShaderCacheKey {
            label: descriptor.label.map(Arc::<str>::from),
            source: Arc::<str>::from(descriptor.source),
        };

        {
            let mut state = self.lock_state();
            let tick = state.tick();
            if let Some(entry) = state.entries.get_mut(&key) {
                entry.last_used = tick;
                let module = entry.module.clone();
                state.hits = state.hits.saturating_add(1);
                return Ok(module);
            }
            state.misses = state.misses.saturating_add(1);
        }

        let compiled = self.compile_uncached(&key).await?;
        let mut state = self.lock_state();
        let tick = state.tick();
        if let Some(entry) = state.entries.get_mut(&key) {
            entry.last_used = tick;
            return Ok(entry.module.clone());
        }

        if state.entries.len() == self.capacity.get() {
            let evicted_key = state
                .entries
                .iter()
                .min_by_key(|(cache_key, entry)| (entry.last_used, *cache_key))
                .map(|(cache_key, _)| cache_key.clone());
            if let Some(evicted_key) = evicted_key {
                state.entries.remove(&evicted_key);
                state.evictions = state.evictions.saturating_add(1);
            }
        }
        state.entries.insert(
            key,
            ShaderCacheEntry {
                module: compiled.clone(),
                last_used: tick,
            },
        );
        Ok(compiled)
    }

    /// Returns current cache counters.
    #[must_use]
    pub fn stats(&self) -> ShaderCacheStats {
        let state = self.lock_state();
        ShaderCacheStats {
            entries: state.entries.len(),
            hits: state.hits,
            misses: state.misses,
            evictions: state.evictions,
        }
    }

    /// Drops cache ownership of every compiled module.
    pub fn clear(&self) {
        self.lock_state().entries.clear();
    }

    fn lock_state(&self) -> MutexGuard<'_, ShaderCacheState> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    async fn compile_uncached(&self, key: &ShaderCacheKey) -> Result<GpuShaderModule> {
        let source = key.source.as_ref();
        let digest: [u8; 32] = Sha256::digest(source.as_bytes()).into();
        let reflection = analyze_wgsl(&self.resources, source, digest, key.label.as_deref())?;
        let device = self.resources.wgpu_device();
        let _scope_guard = self.resources.device().lock_error_scopes().await;

        device.push_error_scope(wgpu::ErrorFilter::Internal);
        device.push_error_scope(wgpu::ErrorFilter::OutOfMemory);
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let raw = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: key.label.as_deref(),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(source)),
        });
        let compilation_info = raw.get_compilation_info().await;
        let validation_error = device.pop_error_scope().await;
        let memory_error = device.pop_error_scope().await;
        let internal_error = device.pop_error_scope().await;

        if let Some(error) = validation_error.or(memory_error).or(internal_error) {
            return Err(wgpu_error(
                error,
                "compile_wgsl",
                digest,
                key.label.as_deref(),
            ));
        }

        let diagnostics = compilation_info
            .messages
            .into_iter()
            .map(|message| ShaderDiagnostic {
                severity: match message.message_type {
                    wgpu::CompilationMessageType::Error => ShaderDiagnosticSeverity::Error,
                    wgpu::CompilationMessageType::Warning => ShaderDiagnosticSeverity::Warning,
                    wgpu::CompilationMessageType::Info => ShaderDiagnosticSeverity::Info,
                },
                message: message.message,
                location: message.location.map(ShaderSourceLocation::from),
            })
            .collect::<Vec<_>>();
        if let Some(diagnostic) = diagnostics
            .iter()
            .find(|item| item.severity == ShaderDiagnosticSeverity::Error)
        {
            return Err(diagnostic_error(
                diagnostic,
                "compile_wgsl",
                digest,
                key.label.as_deref(),
            ));
        }

        let lease = self
            .resources
            .lease(GpuResourceKind::ShaderModule, key.label.as_deref())?;
        Ok(GpuShaderModule(Arc::new(GpuShaderModuleInner {
            lease,
            raw,
            info: GpuShaderModuleInfo {
                label: key.label.as_deref().map(str::to_owned),
                source_digest: digest,
                source_bytes: source.len(),
                reflection,
                diagnostics,
            },
        })))
    }
}

fn validate_source_bounds(descriptor: GpuShaderModuleDescriptor<'_>) -> Result<()> {
    if descriptor.source.trim().is_empty() {
        return Err(shader_input_error(
            "compile_wgsl",
            "WGSL source must not be empty",
            descriptor.label,
        ));
    }
    if descriptor.source.len() > MAX_WGSL_SOURCE_BYTES {
        return Err(shader_input_error(
            "compile_wgsl",
            format!("WGSL source exceeds the {MAX_WGSL_SOURCE_BYTES}-byte module limit"),
            descriptor.label,
        ));
    }
    Ok(())
}

fn analyze_wgsl(
    resources: &GpuResources<'_>,
    source: &str,
    digest: [u8; 32],
    label: Option<&str>,
) -> Result<ShaderReflection> {
    let module = wgpu::naga::front::wgsl::parse_str(source).map_err(|error| {
        let location = error.location(source).map(ShaderSourceLocation::from);
        analysis_error(error.to_string(), "parse_wgsl", digest, label, location)
    })?;
    let mut validator = wgpu::naga::valid::Validator::new(
        wgpu::naga::valid::ValidationFlags::all(),
        naga_capabilities(resources),
    );
    let info = validator.validate(&module).map_err(|error| {
        let location = error
            .spans()
            .next()
            .map(|(span, _)| ShaderSourceLocation::from(span.location(source)));
        analysis_error(error.to_string(), "validate_wgsl", digest, label, location)
    })?;
    Ok(reflect(&module, &info))
}

fn naga_capabilities(resources: &GpuResources<'_>) -> wgpu::naga::valid::Capabilities {
    use wgpu::naga::valid::Capabilities as Caps;

    let features = resources.device().enabled_features();
    let downlevel = resources
        .device()
        .adapter()
        .capabilities()
        .downlevel()
        .flags;
    let mut caps = Caps::empty();
    caps.set(
        Caps::PUSH_CONSTANT,
        features.contains(wgpu::Features::PUSH_CONSTANTS),
    );
    caps.set(Caps::FLOAT64, features.contains(wgpu::Features::SHADER_F64));
    caps.set(
        Caps::PRIMITIVE_INDEX,
        features.contains(wgpu::Features::SHADER_PRIMITIVE_INDEX),
    );
    caps.set(
        Caps::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING,
        features.contains(
            wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING,
        ),
    );
    caps.set(
        Caps::UNIFORM_BUFFER_AND_STORAGE_TEXTURE_ARRAY_NON_UNIFORM_INDEXING,
        features.contains(
            wgpu::Features::UNIFORM_BUFFER_AND_STORAGE_TEXTURE_ARRAY_NON_UNIFORM_INDEXING,
        ),
    );
    caps.set(
        Caps::SAMPLER_NON_UNIFORM_INDEXING,
        features.contains(
            wgpu::Features::SAMPLED_TEXTURE_AND_STORAGE_BUFFER_ARRAY_NON_UNIFORM_INDEXING,
        ),
    );
    caps.set(
        Caps::STORAGE_TEXTURE_16BIT_NORM_FORMATS,
        features.contains(wgpu::Features::TEXTURE_FORMAT_16BIT_NORM),
    );
    caps.set(
        Caps::MULTIVIEW,
        features.contains(wgpu::Features::MULTIVIEW),
    );
    caps.set(
        Caps::EARLY_DEPTH_TEST,
        features.contains(wgpu::Features::SHADER_EARLY_DEPTH_TEST),
    );
    caps.set(
        Caps::SHADER_INT64,
        features.contains(wgpu::Features::SHADER_INT64),
    );
    caps.set(
        Caps::SHADER_INT64_ATOMIC_MIN_MAX,
        features.intersects(
            wgpu::Features::SHADER_INT64_ATOMIC_MIN_MAX
                | wgpu::Features::SHADER_INT64_ATOMIC_ALL_OPS,
        ),
    );
    caps.set(
        Caps::SHADER_INT64_ATOMIC_ALL_OPS,
        features.contains(wgpu::Features::SHADER_INT64_ATOMIC_ALL_OPS),
    );
    caps.set(
        Caps::MULTISAMPLED_SHADING,
        downlevel.contains(wgpu::DownlevelFlags::MULTISAMPLED_SHADING),
    );
    caps.set(
        Caps::DUAL_SOURCE_BLENDING,
        features.contains(wgpu::Features::DUAL_SOURCE_BLENDING),
    );
    caps.set(
        Caps::CUBE_ARRAY_TEXTURES,
        downlevel.contains(wgpu::DownlevelFlags::CUBE_ARRAY_TEXTURES),
    );
    caps.set(
        Caps::SUBGROUP,
        features.intersects(wgpu::Features::SUBGROUP | wgpu::Features::SUBGROUP_VERTEX),
    );
    caps.set(
        Caps::SUBGROUP_BARRIER,
        features.contains(wgpu::Features::SUBGROUP_BARRIER),
    );
    caps.set(
        Caps::SUBGROUP_VERTEX_STAGE,
        features.contains(wgpu::Features::SUBGROUP_VERTEX),
    );
    caps
}

fn reflect(module: &wgpu::naga::Module, info: &wgpu::naga::valid::ModuleInfo) -> ShaderReflection {
    let entry_points = module
        .entry_points
        .iter()
        .map(|entry| ShaderEntryPoint {
            name: entry.name.clone(),
            stage: entry.stage.into(),
            workgroup_size: (entry.stage == wgpu::naga::ShaderStage::Compute)
                .then_some(entry.workgroup_size),
        })
        .collect::<Vec<_>>();

    let mut bindings = module
        .global_variables
        .iter()
        .filter_map(|(handle, variable)| {
            let resource = variable.binding.as_ref()?;
            let mut visibility = wgpu::ShaderStages::empty();
            for (index, entry) in module.entry_points.iter().enumerate() {
                if !info.get_entry_point(index)[handle].is_empty() {
                    visibility |= ShaderStage::from(entry.stage).visibility();
                }
            }
            let (kind, array_count) = binding_kind(module, variable);
            Some(ShaderBinding {
                group: resource.group,
                binding: resource.binding,
                name: variable.name.clone(),
                visibility,
                kind,
                array_count,
            })
        })
        .collect::<Vec<_>>();
    bindings.sort_by_key(|binding| (binding.group, binding.binding));

    let overrides = module
        .overrides
        .iter()
        .map(|(_, item)| ShaderOverride {
            name: item.name.clone(),
            id: item.id,
            has_default: item.init.is_some(),
        })
        .collect();
    ShaderReflection {
        entry_points,
        bindings,
        overrides,
    }
}

fn binding_kind(
    module: &wgpu::naga::Module,
    variable: &wgpu::naga::GlobalVariable,
) -> (ShaderBindingKind, Option<NonZeroU32>) {
    let mut inner = &module.types[variable.ty].inner;
    let mut array_count = None;
    if let wgpu::naga::TypeInner::BindingArray { base, size } = inner {
        inner = &module.types[*base].inner;
        if let wgpu::naga::ArraySize::Constant(count) = size {
            array_count = Some(*count);
        }
    }

    let kind = match variable.space {
        wgpu::naga::AddressSpace::Uniform => ShaderBindingKind::UniformBuffer,
        wgpu::naga::AddressSpace::Storage { access } => ShaderBindingKind::StorageBuffer {
            read_only: !access.contains(wgpu::naga::StorageAccess::STORE),
        },
        wgpu::naga::AddressSpace::Handle => match inner {
            wgpu::naga::TypeInner::Sampler { comparison } => ShaderBindingKind::Sampler {
                comparison: *comparison,
            },
            wgpu::naga::TypeInner::Image {
                dim,
                arrayed,
                class,
            } => match class {
                wgpu::naga::ImageClass::Sampled { kind, multi } => {
                    ShaderBindingKind::SampledTexture {
                        dimension: *dim,
                        arrayed: *arrayed,
                        sample_kind: Some(*kind),
                        multisampled: *multi,
                    }
                }
                wgpu::naga::ImageClass::Depth { multi } => ShaderBindingKind::SampledTexture {
                    dimension: *dim,
                    arrayed: *arrayed,
                    sample_kind: None,
                    multisampled: *multi,
                },
                wgpu::naga::ImageClass::Storage { format, access } => {
                    ShaderBindingKind::StorageTexture {
                        dimension: *dim,
                        arrayed: *arrayed,
                        format: *format,
                        access: storage_access(*access),
                    }
                }
            },
            wgpu::naga::TypeInner::AccelerationStructure => {
                ShaderBindingKind::AccelerationStructure
            }
            _ => ShaderBindingKind::Other,
        },
        _ => ShaderBindingKind::Other,
    };
    (kind, array_count)
}

fn storage_access(access: wgpu::naga::StorageAccess) -> ShaderStorageAccess {
    match (
        access.contains(wgpu::naga::StorageAccess::LOAD),
        access.contains(wgpu::naga::StorageAccess::STORE),
    ) {
        (true, false) => ShaderStorageAccess::ReadOnly,
        (false, true) => ShaderStorageAccess::WriteOnly,
        (true, true) => ShaderStorageAccess::ReadWrite,
        (false, false) => ShaderStorageAccess::None,
    }
}

fn shader_input_error(
    operation: &'static str,
    message: impl Into<String>,
    label: Option<&str>,
) -> Error {
    let mut context = ErrorContext::new(COMPONENT, operation);
    if let Some(label) = label {
        context.insert_field("label", label);
    }
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(context)
}

fn analysis_error(
    message: String,
    operation: &'static str,
    digest: [u8; 32],
    label: Option<&str>,
    location: Option<ShaderSourceLocation>,
) -> Error {
    let mut context = shader_context(operation, digest, label);
    if let Some(location) = location {
        context.insert_field("line", location.line().to_string());
        context.insert_field("column", location.column().to_string());
    }
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(context)
}

fn diagnostic_error(
    diagnostic: &ShaderDiagnostic,
    operation: &'static str,
    digest: [u8; 32],
    label: Option<&str>,
) -> Error {
    let mut context = shader_context(operation, digest, label);
    if let Some(location) = diagnostic.location {
        context.insert_field("line", location.line().to_string());
        context.insert_field("column", location.column().to_string());
    }
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        diagnostic.message.clone(),
    )
    .with_context(context)
}

pub(crate) fn wgpu_error(
    error: wgpu::Error,
    operation: &'static str,
    digest: [u8; 32],
    label: Option<&str>,
) -> Error {
    let (category, recoverability, message) = match error {
        wgpu::Error::OutOfMemory { .. } => (
            ErrorCategory::ResourceExhausted,
            Recoverability::Retryable,
            "GPU memory was exhausted during shader or pipeline compilation".to_owned(),
        ),
        wgpu::Error::Validation { description, .. } => (
            ErrorCategory::InvalidInput,
            Recoverability::UserCorrectable,
            description,
        ),
        wgpu::Error::Internal { description, .. } => (
            ErrorCategory::Internal,
            Recoverability::Retryable,
            description,
        ),
    };
    Error::new(category, recoverability, message)
        .with_context(shader_context(operation, digest, label))
}

pub(crate) fn shader_context(
    operation: &'static str,
    digest: [u8; 32],
    label: Option<&str>,
) -> ErrorContext {
    let mut context =
        ErrorContext::new(COMPONENT, operation).with_field("source_sha256", digest_hex(digest));
    if let Some(label) = label {
        context.insert_field("label", label);
    }
    context
}

fn digest_hex(digest: [u8; 32]) -> String {
    let mut output = String::with_capacity(64);
    for byte in digest {
        let _ = write!(output, "{byte:02x}");
    }
    output
}
