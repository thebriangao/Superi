//! Versioned persistent storage for final-frame and intermediate graph values.
//!
//! Entries are immutable, content-addressed by the complete [`FrameCacheKey`], and partitioned by
//! project, retention tier, cache format, and caller-owned value schema. A write is published from
//! a synced same-directory temporary file, so readers never consume a partially written final
//! entry. Reads are bounded before allocation and validate the complete envelope plus payload
//! digest. Invalid entries are quarantined and reported as classified diagnostics while the graph
//! evaluator receives an ordinary miss and produces the unchanged fresh value.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::marker::PhantomData;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use sha2::{Digest, Sha256};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::ids::ProjectId;
use superi_graph::eval::{EvaluationCacheEntryKind, EvaluationCacheIdentity, EvaluationValueCache};
use superi_image::metadata::ColorPipelineMetadata;

use crate::key::{
    FrameCacheKey, FrameCacheKeyInputs, MediaCacheIdentity, ParameterStateFingerprint,
    RenderSettingsFingerprint,
};

/// Current binary envelope revision for persistent cache entries.
pub const PERSISTENT_CACHE_FORMAT_REVISION: u32 = 1;

const COMPONENT: &str = "superi-cache.disk";
const MAGIC: &[u8; 8] = b"SUPCACHE";
const HEADER_LEN: usize = 124;
const FORMAT_REVISION_OFFSET: usize = 8;
const KIND_OFFSET: usize = 12;
const RESERVED_RANGE: std::ops::Range<usize> = 13..16;
const KEY_RANGE: std::ops::Range<usize> = 16..48;
const SCHEMA_DIGEST_RANGE: std::ops::Range<usize> = 48..80;
const SCHEMA_REVISION_OFFSET: usize = 80;
const PAYLOAD_LEN_OFFSET: usize = 84;
const PAYLOAD_DIGEST_RANGE: std::ops::Range<usize> = 92..124;
const SCHEMA_DOMAIN: &[u8] = b"superi.cache.disk-value-schema.v1\0";
const MAX_SCHEMA_ID_BYTES: usize = 128;
const TEMP_ATTEMPTS: usize = 64;

static NEXT_TEMP_FILE: AtomicU64 = AtomicU64::new(1);
static NEXT_QUARANTINE_FILE: AtomicU64 = AtomicU64::new(1);

/// Filesystem root and per-entry allocation bound for one persistent cache.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiskCacheConfig {
    root: PathBuf,
    max_entry_bytes: u64,
}

impl DiskCacheConfig {
    /// Validates a persistent-cache root and nonzero maximum decoded payload size.
    pub fn new(root: impl Into<PathBuf>, max_entry_bytes: u64) -> Result<Self> {
        let root = root.into();
        if root.as_os_str().is_empty() {
            return Err(config_error(
                "persistent cache root must not be empty",
                &root,
                max_entry_bytes,
                "empty_root",
            ));
        }
        if max_entry_bytes == 0 || usize::try_from(max_entry_bytes).is_err() {
            return Err(config_error(
                "persistent cache entry bound must fit this platform and be nonzero",
                &root,
                max_entry_bytes,
                "invalid_max_entry_bytes",
            ));
        }
        Ok(Self {
            root,
            max_entry_bytes,
        })
    }

    /// Returns the caller-selected cache root before format and schema partitioning.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the maximum payload bytes one entry may allocate or persist.
    #[must_use]
    pub const fn max_entry_bytes(&self) -> u64 {
        self.max_entry_bytes
    }
}

/// Stable identity and revision for a caller-owned cached-value encoding.
///
/// The identifier is a lowercase namespaced value. Changing byte meaning requires a new revision;
/// changing the identifier denotes a different value family. Both values participate in the entry
/// envelope and directory namespace, so incompatible codecs cannot reinterpret old bytes.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiskCacheValueSchema {
    id: String,
    revision: u32,
    digest: [u8; 32],
}

impl DiskCacheValueSchema {
    /// Creates one validated lowercase namespaced value schema with a nonzero revision.
    pub fn new(id: impl Into<String>, revision: u32) -> Result<Self> {
        let id = id.into();
        if !valid_schema_id(&id) || revision == 0 {
            return Err(Error::new(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "persistent cache value schema must be a bounded lowercase namespace with a nonzero revision",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "create_value_schema")
                    .with_field("schema_id", id)
                    .with_field("schema_revision", revision.to_string()),
            ));
        }

        let mut hasher = Sha256::new();
        hasher.update(SCHEMA_DOMAIN);
        hasher.update((id.len() as u64).to_be_bytes());
        hasher.update(id.as_bytes());
        hasher.update(revision.to_be_bytes());
        Ok(Self {
            id,
            revision,
            digest: hasher.finalize().into(),
        })
    }

    /// Returns the stable namespaced value-family identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Returns the caller-owned value encoding revision.
    #[must_use]
    pub const fn revision(&self) -> u32 {
        self.revision
    }

    const fn digest(&self) -> &[u8; 32] {
        &self.digest
    }
}

/// Caller-owned canonical encoder and decoder for one retained evaluator value type.
///
/// Equal values under one complete cache key must encode to byte-compatible meaning. The schema
/// revision must change before the codec changes incompatible byte interpretation.
pub trait DiskCacheCodec<V>: Send + Sync {
    /// Returns the stable value schema used by this codec.
    fn schema(&self) -> &DiskCacheValueSchema;

    /// Encodes one final-frame or intermediate value into bounded persistent bytes.
    fn encode(&self, kind: EvaluationCacheEntryKind, value: &V) -> Result<Vec<u8>>;

    /// Decodes one already integrity-checked payload for the requested retention tier.
    fn decode(&self, kind: EvaluationCacheEntryKind, bytes: &[u8]) -> Result<V>;
}

/// Caller-owned non-graph identity for one persistent cached evaluation scope.
#[derive(Clone, Copy, Debug)]
pub struct FrameDiskCacheContext<'a> {
    project_id: ProjectId,
    media: &'a [MediaCacheIdentity],
    parameters: ParameterStateFingerprint,
    color: &'a ColorPipelineMetadata,
    render_settings: RenderSettingsFingerprint,
}

impl<'a> FrameDiskCacheContext<'a> {
    /// Creates the project and complete outer result identity for one disk-cache scope.
    #[must_use]
    pub const fn new(
        project_id: ProjectId,
        media: &'a [MediaCacheIdentity],
        parameters: ParameterStateFingerprint,
        color: &'a ColorPipelineMetadata,
        render_settings: RenderSettingsFingerprint,
    ) -> Self {
        Self {
            project_id,
            media,
            parameters,
            color,
            render_settings,
        }
    }

    fn entry_key(self, identity: EvaluationCacheIdentity) -> DiskEntryKey {
        DiskEntryKey {
            project_id: self.project_id,
            frame_key: FrameCacheKey::derive(FrameCacheKeyInputs::new(
                self.media,
                identity.graph_key(),
                self.parameters,
                self.color,
                identity.evaluation_key().frame(),
                self.render_settings,
            )),
        }
    }
}

/// Thread-safe persistent final-frame and intermediate-node value storage.
///
/// Graph-facing lookup and insertion never return persistence failures to evaluation. Instead, the
/// adapter records exact shared errors and turns failures into an ordinary miss or skipped write,
/// preserving the freshly evaluated value. Call [`Self::take_diagnostics`] to consume those errors.
pub struct FrameDiskCache<V> {
    root: PathBuf,
    max_entry_bytes: u64,
    schema: DiskCacheValueSchema,
    codec: Box<dyn DiskCacheCodec<V>>,
    io_lock: Mutex<()>,
    diagnostics: Mutex<Vec<Error>>,
    value: PhantomData<fn() -> V>,
}

impl<V> FrameDiskCache<V> {
    /// Creates the versioned schema namespace and an empty diagnostic queue.
    pub fn new<C>(config: DiskCacheConfig, codec: C) -> Result<Self>
    where
        C: DiskCacheCodec<V> + 'static,
    {
        let schema = codec.schema().clone();
        let root = namespace_root(config.root(), &schema);
        fs::create_dir_all(&root).map_err(|source| {
            io_error(
                "create_namespace",
                "could not create persistent cache namespace",
                &root,
                source,
            )
            .with_context(
                ErrorContext::new(COMPONENT, "create_cache")
                    .with_field("schema_id", schema.id())
                    .with_field("schema_revision", schema.revision().to_string())
                    .with_field(
                        "cache_format_revision",
                        PERSISTENT_CACHE_FORMAT_REVISION.to_string(),
                    ),
            )
        })?;
        Ok(Self {
            root,
            max_entry_bytes: config.max_entry_bytes(),
            schema,
            codec: Box::new(codec),
            io_lock: Mutex::new(()),
            diagnostics: Mutex::new(Vec::new()),
            value: PhantomData,
        })
    }

    /// Returns the concrete format and value-schema namespace used for entries.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Returns the number of unconsumed persistence diagnostics.
    #[must_use]
    pub fn diagnostic_len(&self) -> usize {
        lock_or_recover(&self.diagnostics).len()
    }

    /// Drains persistence diagnostics in the order operations recorded them.
    pub fn take_diagnostics(&self) -> Vec<Error> {
        std::mem::take(&mut *lock_or_recover(&self.diagnostics))
    }

    /// Binds one project and complete outer result identity to graph-driven disk reuse.
    #[must_use]
    pub const fn scope<'cache, 'context>(
        &'cache self,
        context: FrameDiskCacheContext<'context>,
    ) -> FrameDiskCacheScope<'cache, 'context, V> {
        FrameDiskCacheScope {
            cache: self,
            context,
        }
    }

    fn record(&self, error: Error) {
        lock_or_recover(&self.diagnostics).push(error);
    }

    fn load(&self, kind: EvaluationCacheEntryKind, key: DiskEntryKey) -> Result<Option<V>> {
        let path = self.entry_path(kind, key);
        let payload = {
            let _guard = lock_or_recover(&self.io_lock);
            match read_payload(
                &path,
                kind,
                key.frame_key,
                &self.schema,
                self.max_entry_bytes,
            ) {
                Ok(payload) => payload,
                Err(fault @ ReadFault::Io { .. }) => {
                    return Err(self.read_fault_error(kind, key, &path, fault));
                }
                Err(fault) => {
                    let recovery = recover_entry(&path);
                    return Err(self.recovered_error(kind, key, &path, fault, recovery));
                }
            }
        };
        let Some(payload) = payload else {
            return Ok(None);
        };

        match self.codec.decode(kind, &payload) {
            Ok(value) => Ok(Some(value)),
            Err(mut source) if source.category() != ErrorCategory::CorruptData => {
                source.push_context(
                    self.entry_context(kind, key, &path, "decode_entry", "codec_decode_failed")
                        .with_field("action", "entry_retained"),
                );
                Err(source)
            }
            Err(source) => {
                let recovery = {
                    let _guard = lock_or_recover(&self.io_lock);
                    match read_payload(
                        &path,
                        kind,
                        key.frame_key,
                        &self.schema,
                        self.max_entry_bytes,
                    ) {
                        Ok(Some(current)) if current == payload => recover_entry(&path),
                        Ok(Some(_)) => Recovery::Changed,
                        Ok(None) => Recovery::AlreadyAbsent,
                        Err(ReadFault::Invalid { .. }) => recover_entry(&path),
                        Err(ReadFault::Io { source, .. }) => Recovery::Failed(source),
                        Err(ReadFault::Decode(_)) => {
                            unreachable!("raw entry reads do not invoke the value codec")
                        }
                    }
                };
                Err(self.recovered_error(kind, key, &path, ReadFault::Decode(source), recovery))
            }
        }
    }

    fn store(&self, kind: EvaluationCacheEntryKind, key: DiskEntryKey, value: &V) -> Result<()> {
        let path = self.entry_path(kind, key);
        let payload = self.codec.encode(kind, value).map_err(|mut error| {
            error.push_context(self.entry_context(
                kind,
                key,
                &path,
                "encode_entry",
                "codec_encode_failed",
            ));
            error
        })?;
        let payload_len = u64::try_from(payload.len()).map_err(|_| {
            self.entry_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
                "persistent cache payload length does not fit the supported format",
                kind,
                key,
                &path,
                "encode_entry",
                "payload_length_overflow",
            )
        })?;
        if payload_len > self.max_entry_bytes {
            return Err(self.entry_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
                "persistent cache payload exceeds the configured entry bound",
                kind,
                key,
                &path,
                "encode_entry",
                "payload_too_large",
            ));
        }

        let _guard = lock_or_recover(&self.io_lock);
        if path.exists() {
            match read_payload(
                &path,
                kind,
                key.frame_key,
                &self.schema,
                self.max_entry_bytes,
            ) {
                Ok(Some(_)) => return Ok(()),
                Ok(None) => {}
                Err(fault @ ReadFault::Io { .. }) => {
                    return Err(self.read_fault_error(kind, key, &path, fault));
                }
                Err(fault) => {
                    let recovery = recover_entry(&path);
                    let isolated = recovery.isolated();
                    let error = self.recovered_error(kind, key, &path, fault, recovery);
                    if isolated {
                        self.record(error);
                    } else {
                        return Err(error);
                    }
                }
            }
        }

        let parent = path.expect_parent();
        fs::create_dir_all(parent).map_err(|source| {
            self.entry_io_error(
                kind,
                key,
                &path,
                "create_entry_directory",
                "could not create persistent cache entry directory",
                source,
            )
        })?;
        let (mut file, temporary) = self.create_temporary(kind, key, &path)?;
        let header = build_header(kind, key.frame_key, &self.schema, &payload);
        let write_result = file
            .write_all(&header)
            .and_then(|()| file.write_all(&payload))
            .and_then(|()| file.sync_all());
        if let Err(source) = write_result {
            drop(file);
            let _ = fs::remove_file(&temporary);
            return Err(self.entry_io_error(
                kind,
                key,
                &path,
                "write_entry",
                "could not write and synchronize persistent cache entry",
                source,
            ));
        }
        drop(file);

        if let Err(source) = fs::rename(&temporary, &path) {
            let _ = fs::remove_file(&temporary);
            if source.kind() == io::ErrorKind::AlreadyExists && path.exists() {
                return Ok(());
            }
            return Err(self.entry_io_error(
                kind,
                key,
                &path,
                "publish_entry",
                "could not publish synchronized persistent cache entry",
                source,
            ));
        }
        sync_parent(parent).map_err(|source| {
            self.entry_io_error(
                kind,
                key,
                &path,
                "sync_entry_directory",
                "persistent cache entry was published but its directory could not be synchronized",
                source,
            )
        })?;
        Ok(())
    }

    fn entry_path(&self, kind: EvaluationCacheEntryKind, key: DiskEntryKey) -> PathBuf {
        self.root
            .join(hex(key.project_id.to_bytes()))
            .join(kind_directory(kind))
            .join(format!("{}.scache", hex(key.frame_key.as_bytes())))
    }

    fn create_temporary(
        &self,
        kind: EvaluationCacheEntryKind,
        key: DiskEntryKey,
        final_path: &Path,
    ) -> Result<(File, PathBuf)> {
        let parent = final_path.expect_parent();
        let base = final_path
            .file_name()
            .expect("entry path owns a file name")
            .to_string_lossy();
        for _ in 0..TEMP_ATTEMPTS {
            let nonce = NEXT_TEMP_FILE.fetch_add(1, Ordering::Relaxed);
            let temporary = parent.join(format!(".{base}.tmp-{}-{nonce}", std::process::id()));
            let mut options = OpenOptions::new();
            options.write(true).create_new(true);
            #[cfg(unix)]
            options.mode(0o600);
            match options.open(&temporary) {
                Ok(file) => return Ok((file, temporary)),
                Err(source) if source.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(source) => {
                    return Err(self.entry_io_error(
                        kind,
                        key,
                        final_path,
                        "create_temporary_entry",
                        "could not reserve a unique persistent cache temporary file",
                        source,
                    ));
                }
            }
        }
        Err(self.entry_error(
            ErrorCategory::Conflict,
            Recoverability::Retryable,
            "could not reserve a unique persistent cache temporary file",
            kind,
            key,
            final_path,
            "create_temporary_entry",
            "temporary_name_exhausted",
        ))
    }

    fn recovered_error(
        &self,
        kind: EvaluationCacheEntryKind,
        key: DiskEntryKey,
        path: &Path,
        fault: ReadFault,
        recovery: Recovery,
    ) -> Error {
        let reason = fault.reason();
        if let Recovery::IsolatedUnsynced {
            action,
            quarantine_path,
            source,
        } = recovery
        {
            let (category, recoverability) = classify_io(source.kind());
            let mut context = self
                .entry_context(kind, key, path, "recover_entry", reason)
                .with_field("action", action);
            if let Some(quarantine_path) = quarantine_path {
                context.insert_field("quarantine_path", quarantine_path.to_string_lossy());
            }
            return Error::with_source(
                category,
                recoverability,
                "persistent cache entry was isolated but its directory could not be synchronized",
                source,
            )
            .with_context(context);
        }
        if let Recovery::Failed(source) = recovery {
            let mut error = io_error(
                "recover_entry",
                "persistent cache entry could not be isolated after validation failed",
                path,
                source,
            );
            error.push_context(
                self.entry_context(kind, key, path, "recover_entry", reason)
                    .with_field("action", "recovery_failed"),
            );
            return error;
        }

        let (action, quarantine_path) = recovery.details();
        let mut context = self
            .entry_context(kind, key, path, "read_entry", reason)
            .with_field("action", action);
        if let Some(quarantine_path) = quarantine_path {
            context.insert_field("quarantine_path", quarantine_path.to_string_lossy());
        }
        match fault {
            ReadFault::Invalid { category, .. } => Error::new(
                category,
                Recoverability::Degraded,
                "persistent cache entry was isolated and will be recomputed",
            )
            .with_context(context),
            ReadFault::Io { .. } => unreachable!("filesystem I/O faults are never quarantined"),
            ReadFault::Decode(mut source) => {
                source.push_context(context);
                source
            }
        }
    }

    fn read_fault_error(
        &self,
        kind: EvaluationCacheEntryKind,
        key: DiskEntryKey,
        path: &Path,
        fault: ReadFault,
    ) -> Error {
        let ReadFault::Io { operation, source } = fault else {
            unreachable!("only filesystem I/O faults use the direct read error path");
        };
        let (category, recoverability) = classify_io(source.kind());
        Error::with_source(
            category,
            recoverability,
            "persistent cache entry could not be read",
            source,
        )
        .with_context(self.entry_context(
            kind,
            key,
            path,
            operation,
            "filesystem_io_failed",
        ))
    }

    #[allow(clippy::too_many_arguments)]
    fn entry_error(
        &self,
        category: ErrorCategory,
        recoverability: Recoverability,
        message: &'static str,
        kind: EvaluationCacheEntryKind,
        key: DiskEntryKey,
        path: &Path,
        operation: &'static str,
        reason: &'static str,
    ) -> Error {
        Error::new(category, recoverability, message)
            .with_context(self.entry_context(kind, key, path, operation, reason))
    }

    fn entry_io_error(
        &self,
        kind: EvaluationCacheEntryKind,
        key: DiskEntryKey,
        path: &Path,
        operation: &'static str,
        message: &'static str,
        source: io::Error,
    ) -> Error {
        let (category, recoverability) = classify_io(source.kind());
        Error::with_source(category, recoverability, message, source)
            .with_context(self.entry_context(kind, key, path, operation, "filesystem_io_failed"))
    }

    fn entry_context(
        &self,
        kind: EvaluationCacheEntryKind,
        key: DiskEntryKey,
        path: &Path,
        operation: &'static str,
        reason: &'static str,
    ) -> ErrorContext {
        ErrorContext::new(COMPONENT, operation)
            .with_field(
                "cache_format_revision",
                PERSISTENT_CACHE_FORMAT_REVISION.to_string(),
            )
            .with_field("key", key.frame_key.to_string())
            .with_field("kind", kind_code(kind))
            .with_field("path", path.to_string_lossy())
            .with_field("project_id", key.project_id.to_string())
            .with_field("reason", reason)
            .with_field("schema_id", self.schema.id())
            .with_field("schema_revision", self.schema.revision().to_string())
    }
}

/// Graph evaluator adapter for one complete persistent result-identity scope.
pub struct FrameDiskCacheScope<'cache, 'context, V> {
    cache: &'cache FrameDiskCache<V>,
    context: FrameDiskCacheContext<'context>,
}

impl<V> EvaluationValueCache<V> for FrameDiskCacheScope<'_, '_, V> {
    fn get(&self, kind: EvaluationCacheEntryKind, identity: EvaluationCacheIdentity) -> Option<V> {
        let key = self.context.entry_key(identity);
        match self.cache.load(kind, key) {
            Ok(value) => value,
            Err(error) => {
                self.cache.record(error);
                None
            }
        }
    }

    fn insert(&self, kind: EvaluationCacheEntryKind, identity: EvaluationCacheIdentity, value: V) {
        let key = self.context.entry_key(identity);
        if let Err(error) = self.cache.store(kind, key, &value) {
            self.cache.record(error);
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct DiskEntryKey {
    project_id: ProjectId,
    frame_key: FrameCacheKey,
}

enum ReadFault {
    Invalid {
        category: ErrorCategory,
        reason: &'static str,
    },
    Io {
        operation: &'static str,
        source: io::Error,
    },
    Decode(Error),
}

impl ReadFault {
    const fn invalid(category: ErrorCategory, reason: &'static str) -> Self {
        Self::Invalid { category, reason }
    }

    const fn reason(&self) -> &'static str {
        match self {
            Self::Invalid { reason, .. } => reason,
            Self::Io { operation, .. } => operation,
            Self::Decode(_) => "codec_decode_failed",
        }
    }
}

enum Recovery {
    Quarantined(PathBuf),
    Removed,
    AlreadyAbsent,
    Changed,
    IsolatedUnsynced {
        action: &'static str,
        quarantine_path: Option<PathBuf>,
        source: io::Error,
    },
    Failed(io::Error),
}

impl Recovery {
    const fn isolated(&self) -> bool {
        !matches!(self, Self::Failed(_))
    }

    fn details(&self) -> (&'static str, Option<&Path>) {
        match self {
            Self::Quarantined(path) => ("quarantined", Some(path.as_path())),
            Self::Removed => ("removed", None),
            Self::AlreadyAbsent => ("already_absent", None),
            Self::Changed => ("entry_changed_before_recovery", None),
            Self::IsolatedUnsynced { .. } => ("directory_sync_failed", None),
            Self::Failed(_) => ("recovery_failed", None),
        }
    }
}

trait PathExt {
    fn expect_parent(&self) -> &Path;
}

impl PathExt for Path {
    fn expect_parent(&self) -> &Path {
        self.parent()
            .expect("persistent cache entry path owns a parent")
    }
}

fn read_payload(
    path: &Path,
    kind: EvaluationCacheEntryKind,
    key: FrameCacheKey,
    schema: &DiskCacheValueSchema,
    max_entry_bytes: u64,
) -> std::result::Result<Option<Vec<u8>>, ReadFault> {
    let mut file = match File::open(path) {
        Ok(file) => file,
        Err(source) if source.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(source) => {
            return Err(ReadFault::Io {
                operation: "open_entry",
                source,
            });
        }
    };
    let mut header = [0_u8; HEADER_LEN];
    read_exact(&mut file, &mut header, "truncated_header")?;

    if &header[..MAGIC.len()] != MAGIC {
        return Err(ReadFault::invalid(
            ErrorCategory::CorruptData,
            "invalid_magic",
        ));
    }
    let format_revision = u32::from_be_bytes(
        header[FORMAT_REVISION_OFFSET..FORMAT_REVISION_OFFSET + 4]
            .try_into()
            .expect("fixed format revision field"),
    );
    if format_revision != PERSISTENT_CACHE_FORMAT_REVISION {
        return Err(ReadFault::invalid(
            ErrorCategory::Unsupported,
            "unsupported_format_revision",
        ));
    }
    if header[KIND_OFFSET] != kind_byte(kind) {
        return Err(ReadFault::invalid(
            ErrorCategory::CorruptData,
            "entry_kind_mismatch",
        ));
    }
    if header[RESERVED_RANGE].iter().any(|byte| *byte != 0) {
        return Err(ReadFault::invalid(
            ErrorCategory::Unsupported,
            "unsupported_header_flags",
        ));
    }
    if header[KEY_RANGE] != key.as_bytes()[..] {
        return Err(ReadFault::invalid(
            ErrorCategory::CorruptData,
            "entry_key_mismatch",
        ));
    }
    if header[SCHEMA_DIGEST_RANGE] != schema.digest()[..] {
        return Err(ReadFault::invalid(
            ErrorCategory::Unsupported,
            "value_schema_mismatch",
        ));
    }
    let schema_revision = u32::from_be_bytes(
        header[SCHEMA_REVISION_OFFSET..SCHEMA_REVISION_OFFSET + 4]
            .try_into()
            .expect("fixed schema revision field"),
    );
    if schema_revision != schema.revision() {
        return Err(ReadFault::invalid(
            ErrorCategory::Unsupported,
            "value_schema_revision_mismatch",
        ));
    }

    let payload_len = u64::from_be_bytes(
        header[PAYLOAD_LEN_OFFSET..PAYLOAD_LEN_OFFSET + 8]
            .try_into()
            .expect("fixed payload length field"),
    );
    if payload_len > max_entry_bytes || usize::try_from(payload_len).is_err() {
        return Err(ReadFault::invalid(
            ErrorCategory::CorruptData,
            "payload_too_large",
        ));
    }
    let mut payload = vec![0_u8; payload_len as usize];
    read_exact(&mut file, &mut payload, "truncated_payload")?;
    let mut trailing = [0_u8; 1];
    match file.read(&mut trailing) {
        Ok(0) => {}
        Ok(_) => {
            return Err(ReadFault::invalid(
                ErrorCategory::CorruptData,
                "trailing_payload_data",
            ));
        }
        Err(source) => {
            return Err(ReadFault::Io {
                operation: "read_trailing_data",
                source,
            });
        }
    }

    let actual_digest: [u8; 32] = Sha256::digest(&payload).into();
    if header[PAYLOAD_DIGEST_RANGE] != actual_digest[..] {
        return Err(ReadFault::invalid(
            ErrorCategory::CorruptData,
            "payload_digest_mismatch",
        ));
    }
    Ok(Some(payload))
}

fn read_exact(
    file: &mut File,
    bytes: &mut [u8],
    truncated_reason: &'static str,
) -> std::result::Result<(), ReadFault> {
    file.read_exact(bytes).map_err(|source| {
        if source.kind() == io::ErrorKind::UnexpectedEof {
            ReadFault::invalid(ErrorCategory::CorruptData, truncated_reason)
        } else {
            ReadFault::Io {
                operation: "read_entry",
                source,
            }
        }
    })
}

fn build_header(
    kind: EvaluationCacheEntryKind,
    key: FrameCacheKey,
    schema: &DiskCacheValueSchema,
    payload: &[u8],
) -> [u8; HEADER_LEN] {
    let mut header = [0_u8; HEADER_LEN];
    header[..MAGIC.len()].copy_from_slice(MAGIC);
    header[FORMAT_REVISION_OFFSET..FORMAT_REVISION_OFFSET + 4]
        .copy_from_slice(&PERSISTENT_CACHE_FORMAT_REVISION.to_be_bytes());
    header[KIND_OFFSET] = kind_byte(kind);
    header[KEY_RANGE].copy_from_slice(key.as_bytes());
    header[SCHEMA_DIGEST_RANGE].copy_from_slice(schema.digest());
    header[SCHEMA_REVISION_OFFSET..SCHEMA_REVISION_OFFSET + 4]
        .copy_from_slice(&schema.revision().to_be_bytes());
    header[PAYLOAD_LEN_OFFSET..PAYLOAD_LEN_OFFSET + 8]
        .copy_from_slice(&(payload.len() as u64).to_be_bytes());
    let payload_digest: [u8; 32] = Sha256::digest(payload).into();
    header[PAYLOAD_DIGEST_RANGE].copy_from_slice(&payload_digest);
    header
}

fn recover_entry(path: &Path) -> Recovery {
    let nonce = NEXT_QUARANTINE_FILE.fetch_add(1, Ordering::Relaxed);
    let quarantine = path.with_file_name(format!(
        "{}.corrupt-{}-{nonce}",
        path.file_name()
            .expect("entry path owns a file name")
            .to_string_lossy(),
        std::process::id()
    ));
    match fs::rename(path, &quarantine) {
        Ok(()) => match sync_parent(path.expect_parent()) {
            Ok(()) => Recovery::Quarantined(quarantine),
            Err(source) => Recovery::IsolatedUnsynced {
                action: "quarantined_directory_sync_failed",
                quarantine_path: Some(quarantine),
                source,
            },
        },
        Err(source) if source.kind() == io::ErrorKind::NotFound => Recovery::AlreadyAbsent,
        Err(rename_source) => match fs::remove_file(path) {
            Ok(()) => match sync_parent(path.expect_parent()) {
                Ok(()) => Recovery::Removed,
                Err(source) => Recovery::IsolatedUnsynced {
                    action: "removed_directory_sync_failed",
                    quarantine_path: None,
                    source,
                },
            },
            Err(source) if source.kind() == io::ErrorKind::NotFound => Recovery::AlreadyAbsent,
            Err(_) => Recovery::Failed(rename_source),
        },
    }
}

#[cfg(unix)]
fn sync_parent(parent: &Path) -> io::Result<()> {
    File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent(_parent: &Path) -> io::Result<()> {
    Ok(())
}

fn namespace_root(root: &Path, schema: &DiskCacheValueSchema) -> PathBuf {
    root.join(format!("format-v{}", PERSISTENT_CACHE_FORMAT_REVISION))
        .join(format!("{}-r{}", hex(schema.digest()), schema.revision()))
}

fn kind_byte(kind: EvaluationCacheEntryKind) -> u8 {
    match kind {
        EvaluationCacheEntryKind::FinalFrame => 1,
        EvaluationCacheEntryKind::IntermediateNode => 2,
    }
}

fn kind_code(kind: EvaluationCacheEntryKind) -> &'static str {
    match kind {
        EvaluationCacheEntryKind::FinalFrame => "final_frame",
        EvaluationCacheEntryKind::IntermediateNode => "intermediate_node",
    }
}

fn kind_directory(kind: EvaluationCacheEntryKind) -> &'static str {
    match kind {
        EvaluationCacheEntryKind::FinalFrame => "final",
        EvaluationCacheEntryKind::IntermediateNode => "intermediate",
    }
}

fn valid_schema_id(id: &str) -> bool {
    if id.is_empty() || id.len() > MAX_SCHEMA_ID_BYTES || !id.is_ascii() || !id.contains('.') {
        return false;
    }
    id.split('.').all(|segment| {
        let mut bytes = segment.bytes();
        bytes.next().is_some_and(|first| first.is_ascii_lowercase())
            && bytes.all(|byte| {
                byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
            })
    })
}

fn config_error(
    message: &'static str,
    root: &Path,
    max_entry_bytes: u64,
    reason: &'static str,
) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(
        ErrorContext::new(COMPONENT, "create_config")
            .with_field("max_entry_bytes", max_entry_bytes.to_string())
            .with_field("reason", reason)
            .with_field("root", root.to_string_lossy()),
    )
}

fn io_error(
    operation: &'static str,
    message: &'static str,
    path: &Path,
    source: io::Error,
) -> Error {
    let (category, recoverability) = classify_io(source.kind());
    Error::with_source(category, recoverability, message, source).with_context(
        ErrorContext::new(COMPONENT, operation).with_field("path", path.to_string_lossy()),
    )
}

fn classify_io(kind: io::ErrorKind) -> (ErrorCategory, Recoverability) {
    match kind {
        io::ErrorKind::PermissionDenied => (
            ErrorCategory::PermissionDenied,
            Recoverability::UserCorrectable,
        ),
        io::ErrorKind::InvalidInput | io::ErrorKind::InvalidData => {
            (ErrorCategory::InvalidInput, Recoverability::UserCorrectable)
        }
        io::ErrorKind::NotFound => (ErrorCategory::NotFound, Recoverability::UserCorrectable),
        io::ErrorKind::TimedOut | io::ErrorKind::WouldBlock | io::ErrorKind::Interrupted => {
            (ErrorCategory::Unavailable, Recoverability::Retryable)
        }
        _ => (ErrorCategory::Unavailable, Recoverability::Retryable),
    }
}

fn hex(bytes: impl AsRef<[u8]>) -> String {
    let bytes = bytes.as_ref();
    let mut encoded = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        use std::fmt::Write as _;
        write!(&mut encoded, "{byte:02x}").expect("writing to String cannot fail");
    }
    encoded
}

fn lock_or_recover<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}
