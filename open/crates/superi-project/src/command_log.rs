//! Durable bounded records for commands executed through the stable project surface.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};
use superi_core::settings::SemanticVersion;

const COMPONENT: &str = "superi-project.command-log";
const RECORD_ACCOUNTING_OVERHEAD_BYTES: usize = 512;
const MAX_TRANSACTION_ID_BYTES: usize = 256;
const MAX_METHOD_BYTES: usize = 256;

/// Maximum number of command records retained in one project.
pub const MAX_PROJECT_COMMAND_LOG_RECORDS: usize = 4096;
/// Maximum total retained command-log bytes, including bounded metadata accounting.
pub const MAX_PROJECT_COMMAND_LOG_BYTES: usize = 64 * 1024 * 1024;
/// Maximum exact serialized request retained for replay.
pub const MAX_RETAINED_PROJECT_COMMAND_BYTES: usize = 1024 * 1024;

/// Stable command classification retained independently of the public API crate.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProjectCommandRecordKind {
    /// Apply one bounded ordered authored transaction.
    Apply,
    /// Restore one retained before-state.
    Undo,
    /// Restore one retained after-state.
    Redo,
    /// Inspect current state without changing authored content.
    Inspect,
}

impl ProjectCommandRecordKind {
    pub(crate) const fn as_str(self) -> &'static str {
        match self {
            Self::Apply => "apply",
            Self::Undo => "undo",
            Self::Redo => "redo",
            Self::Inspect => "inspect",
        }
    }

    pub(crate) fn parse(value: &str) -> Result<Self> {
        match value {
            "apply" => Ok(Self::Apply),
            "undo" => Ok(Self::Undo),
            "redo" => Ok(Self::Redo),
            "inspect" => Ok(Self::Inspect),
            _ => Err(invalid_log(
                "decode_command_record",
                "stored project command kind is unknown",
            )),
        }
    }
}

/// Whether exact typed request bytes remain available for authorized replay inspection.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ProjectCommandPayloadDisposition {
    /// Exact compact serialized request bytes are retained.
    Retained,
    /// Only length and SHA-256 evidence are retained because the request exceeded the bound.
    DigestOnly,
}

/// Validated request evidence prepared before engine dispatch.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectCommandRecordDraft {
    transaction_id: String,
    method: String,
    request_schema_version: SemanticVersion,
    command_kind: ProjectCommandRecordKind,
    expected_project_revision: u64,
    request_byte_length: u64,
    request_sha256: [u8; 32],
    replay_request: Option<Vec<u8>>,
}

impl ProjectCommandRecordDraft {
    /// Builds deterministic request evidence from exact compact serialized bytes.
    pub fn from_serialized_request(
        transaction_id: impl Into<String>,
        method: impl Into<String>,
        request_schema_version: SemanticVersion,
        command_kind: ProjectCommandRecordKind,
        expected_project_revision: u64,
        request: &[u8],
    ) -> Result<Self> {
        let transaction_id = transaction_id.into();
        let method = method.into();
        validate_label(
            "prepare_command_record",
            "transaction_id",
            &transaction_id,
            MAX_TRANSACTION_ID_BYTES,
        )?;
        validate_label(
            "prepare_command_record",
            "method",
            &method,
            MAX_METHOD_BYTES,
        )?;
        let request_byte_length = u64::try_from(request.len()).map_err(|_| {
            command_log_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::UserCorrectable,
                "prepare_command_record",
                "serialized project command length is not representable",
            )
        })?;
        let request_sha256 = Sha256::digest(request).into();
        let replay_request =
            (request.len() <= MAX_RETAINED_PROJECT_COMMAND_BYTES).then(|| request.to_vec());
        Ok(Self {
            transaction_id,
            method,
            request_schema_version,
            command_kind,
            expected_project_revision,
            request_byte_length,
            request_sha256,
            replay_request,
        })
    }

    /// Returns the caller-owned transaction identity.
    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    /// Returns the permanent method identity.
    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }

    /// Returns the request contract version used to interpret replay bytes.
    #[must_use]
    pub const fn request_schema_version(&self) -> &SemanticVersion {
        &self.request_schema_version
    }

    /// Returns the stable command classification.
    #[must_use]
    pub const fn command_kind(&self) -> ProjectCommandRecordKind {
        self.command_kind
    }

    /// Returns the authored revision fence supplied by the caller.
    #[must_use]
    pub const fn expected_project_revision(&self) -> u64 {
        self.expected_project_revision
    }
}

/// One immutable successfully executed public project command record.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectCommandRecord {
    sequence: u64,
    command_sequence: u64,
    transaction_id: String,
    method: String,
    request_schema_version: SemanticVersion,
    command_kind: ProjectCommandRecordKind,
    expected_project_revision: u64,
    request_byte_length: u64,
    request_sha256: [u8; 32],
    replay_request: Option<Vec<u8>>,
    before_project_revision: u64,
    after_project_revision: u64,
    before_semantic_hash: [u8; 32],
    after_semantic_hash: [u8; 32],
    authored_state_changed: bool,
}

impl ProjectCommandRecord {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn from_persisted_parts(
        sequence: u64,
        command_sequence: u64,
        transaction_id: String,
        method: String,
        request_schema_version: SemanticVersion,
        command_kind: ProjectCommandRecordKind,
        expected_project_revision: u64,
        request_byte_length: u64,
        request_sha256: [u8; 32],
        replay_request: Option<Vec<u8>>,
        before_project_revision: u64,
        after_project_revision: u64,
        before_semantic_hash: [u8; 32],
        after_semantic_hash: [u8; 32],
        authored_state_changed: bool,
    ) -> Result<Self> {
        let record = Self {
            sequence,
            command_sequence,
            transaction_id,
            method,
            request_schema_version,
            command_kind,
            expected_project_revision,
            request_byte_length,
            request_sha256,
            replay_request,
            before_project_revision,
            after_project_revision,
            before_semantic_hash,
            after_semantic_hash,
            authored_state_changed,
        };
        record.validate("decode_command_record")?;
        Ok(record)
    }

    /// Returns the durable project-local sequence.
    #[must_use]
    pub const fn sequence(&self) -> u64 {
        self.sequence
    }

    /// Returns the correlated engine command sequence.
    #[must_use]
    pub const fn command_sequence(&self) -> u64 {
        self.command_sequence
    }

    /// Returns the caller-owned transaction identity.
    #[must_use]
    pub fn transaction_id(&self) -> &str {
        &self.transaction_id
    }

    /// Returns the permanent public method identity.
    #[must_use]
    pub fn method(&self) -> &str {
        &self.method
    }

    /// Returns the request schema version.
    #[must_use]
    pub const fn request_schema_version(&self) -> &SemanticVersion {
        &self.request_schema_version
    }

    /// Returns the stable command kind.
    #[must_use]
    pub const fn command_kind(&self) -> ProjectCommandRecordKind {
        self.command_kind
    }

    /// Returns the caller's authored revision fence.
    #[must_use]
    pub const fn expected_project_revision(&self) -> u64 {
        self.expected_project_revision
    }

    /// Returns the exact serialized request length.
    #[must_use]
    pub const fn request_byte_length(&self) -> u64 {
        self.request_byte_length
    }

    /// Returns the SHA-256 digest of the exact serialized request.
    #[must_use]
    pub const fn request_sha256(&self) -> &[u8; 32] {
        &self.request_sha256
    }

    /// Returns whether exact request bytes or digest-only evidence is retained.
    #[must_use]
    pub const fn payload_disposition(&self) -> ProjectCommandPayloadDisposition {
        if self.replay_request.is_some() {
            ProjectCommandPayloadDisposition::Retained
        } else {
            ProjectCommandPayloadDisposition::DigestOnly
        }
    }

    /// Returns exact compact typed request bytes when retained.
    #[must_use]
    pub fn replay_request(&self) -> Option<&[u8]> {
        self.replay_request.as_deref()
    }

    /// Returns the authored revision before execution.
    #[must_use]
    pub const fn before_project_revision(&self) -> u64 {
        self.before_project_revision
    }

    /// Returns the authored revision after execution.
    #[must_use]
    pub const fn after_project_revision(&self) -> u64 {
        self.after_project_revision
    }

    /// Returns the semantic project hash before execution.
    #[must_use]
    pub const fn before_semantic_hash(&self) -> &[u8; 32] {
        &self.before_semantic_hash
    }

    /// Returns the semantic project hash after execution.
    #[must_use]
    pub const fn after_semantic_hash(&self) -> &[u8; 32] {
        &self.after_semantic_hash
    }

    /// Reports whether authored state changed.
    #[must_use]
    pub const fn authored_state_changed(&self) -> bool {
        self.authored_state_changed
    }

    fn accounted_bytes(&self) -> usize {
        RECORD_ACCOUNTING_OVERHEAD_BYTES
            .saturating_add(self.transaction_id.len())
            .saturating_add(self.method.len())
            .saturating_add(self.replay_request.as_ref().map_or(0, std::vec::Vec::len))
    }

    fn validate(&self, operation: &'static str) -> Result<()> {
        if self.sequence == 0 || self.command_sequence == 0 {
            return Err(invalid_log(
                operation,
                "command record sequences must be nonzero",
            ));
        }
        validate_label(
            operation,
            "transaction_id",
            &self.transaction_id,
            MAX_TRANSACTION_ID_BYTES,
        )?;
        validate_label(operation, "method", &self.method, MAX_METHOD_BYTES)?;
        if self.expected_project_revision != self.before_project_revision {
            return Err(invalid_log(
                operation,
                "command record revision fence does not match its before revision",
            ));
        }
        if !self.authored_state_changed
            && (self.before_project_revision != self.after_project_revision
                || self.before_semantic_hash != self.after_semantic_hash)
        {
            return Err(invalid_log(
                operation,
                "unchanged command record contains changed authored evidence",
            ));
        }
        if self.authored_state_changed
            && self.after_project_revision <= self.before_project_revision
        {
            return Err(invalid_log(
                operation,
                "changed command record does not advance the authored revision",
            ));
        }
        match &self.replay_request {
            Some(request) => {
                if request.len() > MAX_RETAINED_PROJECT_COMMAND_BYTES
                    || request.len() as u64 != self.request_byte_length
                    || <[u8; 32]>::from(Sha256::digest(request)) != self.request_sha256
                {
                    return Err(invalid_log(
                        operation,
                        "retained command request evidence is inconsistent",
                    ));
                }
            }
            None => {
                if self.request_byte_length <= MAX_RETAINED_PROJECT_COMMAND_BYTES as u64 {
                    return Err(invalid_log(
                        operation,
                        "digest-only command request did not exceed the replay byte bound",
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Bounded durable command-log state owned beside authored project state.
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProjectCommandLog {
    next_sequence: u64,
    retained_bytes: u64,
    records: VecDeque<ProjectCommandRecord>,
}

impl Default for ProjectCommandLog {
    fn default() -> Self {
        Self::new()
    }
}

impl ProjectCommandLog {
    /// Creates an empty log whose first successful record receives sequence one.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            next_sequence: 1,
            retained_bytes: 0,
            records: VecDeque::new(),
        }
    }

    pub(crate) fn from_persisted_parts(
        next_sequence: u64,
        records: VecDeque<ProjectCommandRecord>,
    ) -> Result<Self> {
        let retained_bytes = records.iter().try_fold(0_u64, |total, record| {
            total
                .checked_add(record.accounted_bytes() as u64)
                .ok_or_else(|| {
                    invalid_log(
                        "decode_command_log",
                        "command log byte accounting overflowed",
                    )
                })
        })?;
        let log = Self {
            next_sequence,
            retained_bytes,
            records,
        };
        log.validate("decode_command_log")?;
        Ok(log)
    }

    pub(crate) const fn next_sequence(&self) -> u64 {
        self.next_sequence
    }

    /// Returns retained records in project-local sequence order.
    #[must_use]
    pub fn records(&self) -> &VecDeque<ProjectCommandRecord> {
        &self.records
    }

    /// Returns the number of retained records.
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// Reports whether no record is retained.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Returns the oldest replayable metadata sequence, when any record remains.
    #[must_use]
    pub fn oldest_sequence(&self) -> Option<u64> {
        self.records.front().map(ProjectCommandRecord::sequence)
    }

    /// Returns the latest allocated sequence, or zero for a never-recorded project.
    #[must_use]
    pub const fn latest_sequence(&self) -> u64 {
        self.next_sequence - 1
    }

    /// Returns bounded retained byte accounting.
    #[must_use]
    pub const fn retained_bytes(&self) -> u64 {
        self.retained_bytes
    }

    /// Appends one successful execution and applies deterministic oldest-first retention.
    #[allow(clippy::too_many_arguments)]
    pub fn append(
        &mut self,
        draft: ProjectCommandRecordDraft,
        command_sequence: u64,
        before_project_revision: u64,
        after_project_revision: u64,
        before_semantic_hash: [u8; 32],
        after_semantic_hash: [u8; 32],
        authored_state_changed: bool,
    ) -> Result<&ProjectCommandRecord> {
        let sequence = self.next_sequence;
        let next_sequence = sequence.checked_add(1).ok_or_else(|| {
            command_log_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::Terminal,
                "append_command_record",
                "project command-log sequence is exhausted",
            )
        })?;
        let record = ProjectCommandRecord {
            sequence,
            command_sequence,
            transaction_id: draft.transaction_id,
            method: draft.method,
            request_schema_version: draft.request_schema_version,
            command_kind: draft.command_kind,
            expected_project_revision: draft.expected_project_revision,
            request_byte_length: draft.request_byte_length,
            request_sha256: draft.request_sha256,
            replay_request: draft.replay_request,
            before_project_revision,
            after_project_revision,
            before_semantic_hash,
            after_semantic_hash,
            authored_state_changed,
        };
        record.validate("append_command_record")?;
        let record_bytes = u64::try_from(record.accounted_bytes()).map_err(|_| {
            command_log_error(
                ErrorCategory::ResourceExhausted,
                Recoverability::Terminal,
                "append_command_record",
                "project command record accounting is not representable",
            )
        })?;

        self.next_sequence = next_sequence;
        self.retained_bytes = self
            .retained_bytes
            .checked_add(record_bytes)
            .ok_or_else(|| {
                command_log_error(
                    ErrorCategory::ResourceExhausted,
                    Recoverability::Terminal,
                    "append_command_record",
                    "project command-log byte accounting is exhausted",
                )
            })?;
        self.records.push_back(record);
        while self.records.len() > MAX_PROJECT_COMMAND_LOG_RECORDS
            || self.retained_bytes > MAX_PROJECT_COMMAND_LOG_BYTES as u64
        {
            let removed = self
                .records
                .pop_front()
                .expect("a command log over a positive bound contains a record");
            self.retained_bytes = self
                .retained_bytes
                .saturating_sub(removed.accounted_bytes() as u64);
        }
        Ok(self
            .records
            .back()
            .expect("the new bounded command record fits the retention envelope"))
    }

    /// Encodes deterministic strict JSON for project persistence.
    pub fn encode(&self) -> Result<Vec<u8>> {
        self.validate("encode_command_log")?;
        serde_json::to_vec(self).map_err(|error| {
            command_log_error(
                ErrorCategory::Internal,
                Recoverability::Terminal,
                "encode_command_log",
                "project command log could not be encoded",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "encode_command_log")
                    .with_field("source", error.to_string()),
            )
        })
    }

    /// Decodes and validates one strict persisted command log.
    pub fn decode(bytes: &[u8]) -> Result<Self> {
        let log = serde_json::from_slice::<Self>(bytes).map_err(|error| {
            invalid_log(
                "decode_command_log",
                "project command log is not valid strict JSON",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "decode_command_log")
                    .with_field("source", error.to_string()),
            )
        })?;
        log.validate("decode_command_log")?;
        Ok(log)
    }

    fn validate(&self, operation: &'static str) -> Result<()> {
        if self.next_sequence == 0 || self.records.len() > MAX_PROJECT_COMMAND_LOG_RECORDS {
            return Err(invalid_log(
                operation,
                "project command log bounds are invalid",
            ));
        }
        let mut prior_sequence = None;
        let mut retained_bytes = 0_u64;
        for record in &self.records {
            record.validate(operation)?;
            if let Some(prior) = prior_sequence {
                if record.sequence != prior + 1 {
                    return Err(invalid_log(
                        operation,
                        "project command record sequences are not contiguous",
                    ));
                }
            }
            prior_sequence = Some(record.sequence);
            retained_bytes = retained_bytes
                .checked_add(record.accounted_bytes() as u64)
                .ok_or_else(|| invalid_log(operation, "command log byte accounting overflowed"))?;
        }
        if retained_bytes != self.retained_bytes
            || retained_bytes > MAX_PROJECT_COMMAND_LOG_BYTES as u64
        {
            return Err(invalid_log(
                operation,
                "project command log retained byte accounting is inconsistent",
            ));
        }
        if let Some(latest) = prior_sequence {
            if latest.checked_add(1) != Some(self.next_sequence) {
                return Err(invalid_log(
                    operation,
                    "project command log next sequence is inconsistent",
                ));
            }
        } else if self.next_sequence != 1 {
            return Err(invalid_log(
                operation,
                "empty project command log has allocated sequence state",
            ));
        }
        Ok(())
    }
}

fn validate_label(
    operation: &'static str,
    field: &'static str,
    value: &str,
    max_bytes: usize,
) -> Result<()> {
    if value.is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
        return Err(
            invalid_log(operation, "project command record label is invalid")
                .with_context(ErrorContext::new(COMPONENT, operation).with_field("field", field)),
        );
    }
    Ok(())
}

fn invalid_log(operation: &'static str, message: &'static str) -> Error {
    command_log_error(
        ErrorCategory::CorruptData,
        Recoverability::UserCorrectable,
        operation,
        message,
    )
}

fn command_log_error(
    category: ErrorCategory,
    recoverability: Recoverability,
    operation: &'static str,
    message: &'static str,
) -> Error {
    Error::new(category, recoverability, message)
        .with_context(ErrorContext::new(COMPONENT, operation))
}
