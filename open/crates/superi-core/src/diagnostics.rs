//! Shared structured diagnostics, safe error presentation, and performance counters.
//!
//! Diagnostic events own deterministic typed fields so engines, projects,
//! extensions, and automation can inspect the same values. Raw failure snapshots
//! remain internal diagnostic data. [`UserSafeError`] is the separate projection
//! intended for UI, CLI, and automation presentation.

use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};

use crate::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

/// Stable importance assigned to a diagnostic event.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum DiagnosticSeverity {
    /// Fine-grained execution details used for tracing a workflow.
    Trace,
    /// Developer-facing state useful while diagnosing behavior.
    Debug,
    /// A normal lifecycle or operation event.
    Info,
    /// A condition that deserves attention while execution may continue.
    Warning,
    /// A condition that prevented an operation from completing safely.
    Error,
}

impl DiagnosticSeverity {
    /// Every severity defined by this version, in stable code order.
    pub const ALL: &'static [Self] = &[
        Self::Trace,
        Self::Debug,
        Self::Info,
        Self::Warning,
        Self::Error,
    ];

    /// Returns the permanent cross-process code for this severity.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Trace => "trace",
            Self::Debug => "debug",
            Self::Info => "info",
            Self::Warning => "warning",
            Self::Error => "error",
        }
    }

    /// Looks up a severity by its permanent code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "trace" => Some(Self::Trace),
            "debug" => Some(Self::Debug),
            "info" => Some(Self::Info),
            "warning" => Some(Self::Warning),
            "error" => Some(Self::Error),
            _ => None,
        }
    }
}

impl fmt::Display for DiagnosticSeverity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// Controls which presentation boundary may receive a tracing field.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum FieldVisibility {
    /// The field may be presented to a user.
    UserSafe,
    /// The field is available only to internal diagnostic consumers.
    Internal,
    /// The field contains sensitive data and requires explicit handling.
    Sensitive,
}

impl FieldVisibility {
    /// Every visibility defined by this version, in stable code order.
    pub const ALL: &'static [Self] = &[Self::UserSafe, Self::Internal, Self::Sensitive];

    /// Returns the permanent cross-process code for this visibility.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::UserSafe => "user_safe",
            Self::Internal => "internal",
            Self::Sensitive => "sensitive",
        }
    }

    /// Looks up a visibility by its permanent code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "user_safe" => Some(Self::UserSafe),
            "internal" => Some(Self::Internal),
            "sensitive" => Some(Self::Sensitive),
            _ => None,
        }
    }
}

impl fmt::Display for FieldVisibility {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// A finite floating-point value with canonical zero representation.
///
/// NaN and infinity are rejected because they do not have portable, canonical
/// comparison or interchange behavior. Negative zero is normalized to positive
/// zero.
#[derive(Clone, Copy, Debug)]
pub struct FiniteF64(f64);

impl FiniteF64 {
    /// Creates a finite value and normalizes negative zero.
    pub fn new(value: f64) -> Result<Self> {
        if !value.is_finite() {
            return Err(invalid_diagnostic_value("floating_point"));
        }

        let value = if value == 0.0 { 0.0 } else { value };
        Ok(Self(value))
    }

    /// Returns the contained finite value.
    #[must_use]
    pub const fn get(self) -> f64 {
        self.0
    }
}

impl PartialEq for FiniteF64 {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl Eq for FiniteF64 {}

impl TryFrom<f64> for FiniteF64 {
    type Error = Error;

    fn try_from(value: f64) -> Result<Self> {
        Self::new(value)
    }
}

impl From<FiniteF64> for f64 {
    fn from(value: FiniteF64) -> Self {
        value.get()
    }
}

/// An owned typed value recorded on a diagnostic event.
#[derive(Clone, Debug, Eq, PartialEq)]
#[non_exhaustive]
pub enum TraceValue {
    /// A boolean value.
    Boolean(bool),
    /// A signed 64-bit integer.
    Signed(i64),
    /// An unsigned 64-bit integer.
    Unsigned(u64),
    /// A finite 64-bit floating-point value.
    Float(FiniteF64),
    /// Owned UTF-8 text.
    Text(String),
}

impl TraceValue {
    /// Returns the stable type code for this value.
    #[must_use]
    pub const fn kind_code(&self) -> &'static str {
        match self {
            Self::Boolean(_) => "bool",
            Self::Signed(_) => "i64",
            Self::Unsigned(_) => "u64",
            Self::Float(_) => "f64",
            Self::Text(_) => "text",
        }
    }
}

impl From<bool> for TraceValue {
    fn from(value: bool) -> Self {
        Self::Boolean(value)
    }
}

impl From<i64> for TraceValue {
    fn from(value: i64) -> Self {
        Self::Signed(value)
    }
}

impl From<u64> for TraceValue {
    fn from(value: u64) -> Self {
        Self::Unsigned(value)
    }
}

impl From<FiniteF64> for TraceValue {
    fn from(value: FiniteF64) -> Self {
        Self::Float(value)
    }
}

impl From<String> for TraceValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

impl From<&str> for TraceValue {
    fn from(value: &str) -> Self {
        Self::Text(value.to_owned())
    }
}

impl TryFrom<f64> for TraceValue {
    type Error = Error;

    fn try_from(value: f64) -> Result<Self> {
        FiniteF64::new(value).map(Self::Float)
    }
}

/// One typed tracing field and its presentation boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TraceField {
    value: TraceValue,
    visibility: FieldVisibility,
}

impl TraceField {
    // Used by the crate-owned serialization boundary without exposing unchecked
    // construction to public consumers.
    #[allow(dead_code)]
    pub(crate) const fn from_parts(value: TraceValue, visibility: FieldVisibility) -> Self {
        Self { value, visibility }
    }

    /// Creates an internal diagnostic field.
    #[must_use]
    pub fn internal(value: impl Into<TraceValue>) -> Self {
        Self {
            value: value.into(),
            visibility: FieldVisibility::Internal,
        }
    }

    /// Creates a field that may be presented to a user.
    ///
    /// Callers must use this only when the value has already been reviewed for
    /// paths, identifiers, credentials, private media details, and other secrets.
    #[must_use]
    pub fn user_safe(value: impl Into<TraceValue>) -> Self {
        Self {
            value: value.into(),
            visibility: FieldVisibility::UserSafe,
        }
    }

    /// Creates a sensitive diagnostic field that requires explicit handling.
    #[must_use]
    pub fn sensitive(value: impl Into<TraceValue>) -> Self {
        Self {
            value: value.into(),
            visibility: FieldVisibility::Sensitive,
        }
    }

    /// Returns the typed field value.
    #[must_use]
    pub const fn value(&self) -> &TraceValue {
        &self.value
    }

    /// Returns the field's presentation boundary.
    #[must_use]
    pub const fn visibility(&self) -> FieldVisibility {
        self.visibility
    }
}

/// An owned snapshot of a shared error for internal diagnostics.
///
/// The summary, contexts, and source summaries may contain sensitive values.
/// They must never be presented directly to users. Use [`UserSafeError`] for a
/// presentation boundary.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FailureDiagnostic {
    category: ErrorCategory,
    recoverability: Recoverability,
    summary: String,
    contexts: Vec<ErrorContext>,
    source_summaries: Vec<String>,
}

impl FailureDiagnostic {
    // Used by the crate-owned serialization boundary after wire validation.
    #[allow(dead_code)]
    pub(crate) fn from_parts(
        category: ErrorCategory,
        recoverability: Recoverability,
        summary: String,
        contexts: Vec<ErrorContext>,
        source_summaries: Vec<String>,
    ) -> Self {
        Self {
            category,
            recoverability,
            summary,
            contexts,
            source_summaries,
        }
    }

    /// Captures the classification and complete standard source chain.
    #[must_use]
    pub fn from_error(error: &Error) -> Self {
        let mut source_summaries = Vec::new();
        let mut source = StdError::source(error);
        while let Some(current) = source {
            source_summaries.push(current.to_string());
            source = current.source();
        }

        Self {
            category: error.category(),
            recoverability: error.recoverability(),
            summary: error.message().to_owned(),
            contexts: error.contexts().to_vec(),
            source_summaries,
        }
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn category(&self) -> ErrorCategory {
        self.category
    }

    /// Returns the explicit recovery classification.
    #[must_use]
    pub const fn recoverability(&self) -> Recoverability {
        self.recoverability
    }

    /// Returns the raw internal summary.
    #[must_use]
    pub fn summary(&self) -> &str {
        &self.summary
    }

    /// Returns contexts from the failing operation toward outer callers.
    #[must_use]
    pub fn contexts(&self) -> &[ErrorContext] {
        &self.contexts
    }

    /// Returns standard source summaries from the direct source to the leaf.
    #[must_use]
    pub fn source_summaries(&self) -> &[String] {
        &self.source_summaries
    }
}

/// A user-safe default English projection of a shared error.
///
/// This value is derived only from stable category and recoverability enums. It
/// never copies the raw error summary, context, field values, or source chain.
/// Presentation layers may localize it by [`UserSafeError::code`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct UserSafeError {
    category: ErrorCategory,
    recoverability: Recoverability,
    code: String,
    title: &'static str,
    action: &'static str,
}

impl UserSafeError {
    /// Derives a safe projection from a shared error.
    #[must_use]
    pub fn from_error(error: &Error) -> Self {
        Self::from_parts(error.category(), error.recoverability())
    }

    pub(crate) fn from_parts(category: ErrorCategory, recoverability: Recoverability) -> Self {
        let title = match category {
            ErrorCategory::InvalidInput => "Some information is not valid.",
            ErrorCategory::NotFound => "The requested item could not be found.",
            ErrorCategory::Conflict => "The operation conflicts with the current state.",
            ErrorCategory::Unsupported => "This operation is not supported.",
            ErrorCategory::PermissionDenied => "Superi does not have the required permission.",
            ErrorCategory::ResourceExhausted => "A required resource has reached its limit.",
            ErrorCategory::Unavailable => "A required resource is temporarily unavailable.",
            ErrorCategory::Timeout => "The operation took too long.",
            ErrorCategory::Cancelled => "The operation was cancelled.",
            ErrorCategory::CorruptData => "Some data could not be read safely.",
            ErrorCategory::Internal => "Superi could not complete the operation.",
        };
        let action = match recoverability {
            Recoverability::Retryable => {
                "Try the operation again. If it keeps failing, review the diagnostic report."
            }
            Recoverability::Degraded => {
                "You can continue with reduced functionality. Review the diagnostic report to restore the unavailable feature."
            }
            Recoverability::UserCorrectable => {
                "Review the input, permissions, or project state, then try again."
            }
            Recoverability::Terminal => {
                "Save any available work and restart Superi. If the problem returns, share the diagnostic report."
            }
        };

        Self {
            category,
            recoverability,
            code: format!("error.{}.{}", category.code(), recoverability.code()),
            title,
            action,
        }
    }

    /// Returns the stable localization and automation code.
    #[must_use]
    pub fn code(&self) -> &str {
        &self.code
    }

    /// Returns the safe default title.
    #[must_use]
    pub const fn title(&self) -> &'static str {
        self.title
    }

    /// Returns the safe default recovery action.
    #[must_use]
    pub const fn action(&self) -> &'static str {
        self.action
    }

    /// Returns the stable failure category.
    #[must_use]
    pub const fn category(&self) -> ErrorCategory {
        self.category
    }

    /// Returns the explicit recovery classification.
    #[must_use]
    pub const fn recoverability(&self) -> Recoverability {
        self.recoverability
    }
}

impl fmt::Display for UserSafeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} {}", self.title, self.action)
    }
}

/// A structured event shared across Superi processes and public consumers.
///
/// Events do not include an implicit clock or process identifier. Callers record
/// that context explicitly as fields when it is meaningful and safe.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticEvent {
    name: String,
    component: String,
    severity: DiagnosticSeverity,
    message: String,
    fields: BTreeMap<String, TraceField>,
    failure: Option<FailureDiagnostic>,
    user_safe_error: Option<UserSafeError>,
}

impl DiagnosticEvent {
    // Used by the crate-owned serialization boundary. Validation remains here so
    // deserialization cannot bypass the canonical name grammar.
    #[allow(dead_code)]
    pub(crate) fn from_parts(
        name: String,
        component: String,
        severity: DiagnosticSeverity,
        message: String,
        fields: BTreeMap<String, TraceField>,
        failure: Option<FailureDiagnostic>,
        user_safe_error: Option<UserSafeError>,
    ) -> Result<Self> {
        validate_diagnostic_name("event", &name)?;
        validate_diagnostic_name("component", &component)?;
        for field_name in fields.keys() {
            validate_diagnostic_name("field", field_name)?;
        }
        match (&failure, &user_safe_error) {
            (None, None) => {}
            (Some(failure), Some(user_safe_error))
                if failure.category() == user_safe_error.category()
                    && failure.recoverability() == user_safe_error.recoverability() => {}
            _ => return Err(invalid_diagnostic_value("error_projection")),
        }
        Ok(Self {
            name,
            component,
            severity,
            message,
            fields,
            failure,
            user_safe_error,
        })
    }

    /// Creates a regular diagnostic event.
    ///
    /// Names and components use lowercase ASCII letters and digits separated by
    /// `.`, `_`, or `-`. Separators may not lead, trail, or repeat.
    pub fn new(
        name: impl Into<String>,
        component: impl Into<String>,
        severity: DiagnosticSeverity,
        message: impl Into<String>,
    ) -> Result<Self> {
        let name = name.into();
        validate_diagnostic_name("event", &name)?;
        let component = component.into();
        validate_diagnostic_name("component", &component)?;

        Ok(Self {
            name,
            component,
            severity,
            message: message.into(),
            fields: BTreeMap::new(),
            failure: None,
            user_safe_error: None,
        })
    }

    /// Creates an event that owns internal and user-safe projections of an error.
    pub fn from_error(
        name: impl Into<String>,
        component: impl Into<String>,
        error: &Error,
    ) -> Result<Self> {
        let severity = match error.recoverability() {
            Recoverability::Retryable
            | Recoverability::Degraded
            | Recoverability::UserCorrectable => DiagnosticSeverity::Warning,
            Recoverability::Terminal => DiagnosticSeverity::Error,
        };
        let mut event = Self::new(name, component, severity, error.message())?;
        event.failure = Some(FailureDiagnostic::from_error(error));
        event.user_safe_error = Some(UserSafeError::from_error(error));
        Ok(event)
    }

    /// Adds a tracing field and returns the updated event.
    pub fn with_field(mut self, name: impl Into<String>, field: TraceField) -> Result<Self> {
        self.insert_field(name, field)?;
        Ok(self)
    }

    /// Inserts a tracing field, returning the previous field for the same name.
    pub fn insert_field(
        &mut self,
        name: impl Into<String>,
        field: TraceField,
    ) -> Result<Option<TraceField>> {
        let name = name.into();
        validate_diagnostic_name("field", &name)?;
        Ok(self.fields.insert(name, field))
    }

    /// Returns the stable event name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the owning component.
    #[must_use]
    pub fn component(&self) -> &str {
        &self.component
    }

    /// Returns the event severity.
    #[must_use]
    pub const fn severity(&self) -> DiagnosticSeverity {
        self.severity
    }

    /// Returns the raw diagnostic summary.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns all tracing fields in deterministic name order.
    #[must_use]
    pub const fn fields(&self) -> &BTreeMap<String, TraceField> {
        &self.fields
    }

    /// Returns one tracing field by name.
    #[must_use]
    pub fn field(&self, name: &str) -> Option<&TraceField> {
        self.fields.get(name)
    }

    /// Iterates only fields explicitly approved for user presentation.
    pub fn user_safe_fields(&self) -> impl Iterator<Item = (&str, &TraceValue)> {
        self.fields.iter().filter_map(|(name, field)| {
            (field.visibility() == FieldVisibility::UserSafe)
                .then_some((name.as_str(), field.value()))
        })
    }

    /// Returns the owned internal failure snapshot for an error event.
    #[must_use]
    pub const fn failure(&self) -> Option<&FailureDiagnostic> {
        self.failure.as_ref()
    }

    /// Returns the safe user-facing projection for an error event.
    #[must_use]
    pub const fn user_safe_error(&self) -> Option<&UserSafeError> {
        self.user_safe_error.as_ref()
    }
}

/// Stable unit attached to a performance counter.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum CounterUnit {
    /// Dimensionless occurrences.
    Count,
    /// Bytes of storage or transfer.
    Bytes,
    /// Elapsed nanoseconds.
    Nanoseconds,
    /// Video or image frames.
    Frames,
    /// Audio samples.
    Samples,
}

impl CounterUnit {
    /// Every unit defined by this version, in stable code order.
    pub const ALL: &'static [Self] = &[
        Self::Count,
        Self::Bytes,
        Self::Nanoseconds,
        Self::Frames,
        Self::Samples,
    ];

    /// Returns the permanent cross-process code for this unit.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Count => "count",
            Self::Bytes => "bytes",
            Self::Nanoseconds => "nanoseconds",
            Self::Frames => "frames",
            Self::Samples => "samples",
        }
    }

    /// Looks up a counter unit by its permanent code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "count" => Some(Self::Count),
            "bytes" => Some(Self::Bytes),
            "nanoseconds" => Some(Self::Nanoseconds),
            "frames" => Some(Self::Frames),
            "samples" => Some(Self::Samples),
            _ => None,
        }
    }
}

impl fmt::Display for CounterUnit {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// An owned transportable performance counter value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CounterSnapshot {
    name: String,
    unit: CounterUnit,
    value: u64,
}

impl CounterSnapshot {
    /// Returns the stable counter name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the counter unit.
    #[must_use]
    pub const fn unit(&self) -> CounterUnit {
        self.unit
    }

    /// Returns the captured value.
    #[must_use]
    pub const fn value(&self) -> u64 {
        self.value
    }
}

/// A thread-safe monotonic performance counter.
///
/// The counter saturates at `u64::MAX` and has no reset operation, so it cannot
/// wrap or move backward during its lifetime. Relaxed atomic ordering is used
/// because the value measures work and never synchronizes unrelated state.
pub struct PerformanceCounter {
    name: String,
    unit: CounterUnit,
    value: AtomicU64,
}

impl PerformanceCounter {
    /// Creates a zero-valued counter.
    pub fn new(name: impl Into<String>, unit: CounterUnit) -> Result<Self> {
        Self::with_initial_value(name, unit, 0)
    }

    /// Creates a counter with an initial monotonic value.
    pub fn with_initial_value(
        name: impl Into<String>,
        unit: CounterUnit,
        initial_value: u64,
    ) -> Result<Self> {
        let name = name.into();
        validate_diagnostic_name("counter", &name)?;
        Ok(Self {
            name,
            unit,
            value: AtomicU64::new(initial_value),
        })
    }

    /// Increments the counter by one and returns its new value.
    pub fn increment(&self) -> u64 {
        self.add(1)
    }

    /// Adds an amount with saturation and returns the new value.
    pub fn add(&self, amount: u64) -> u64 {
        let previous = self
            .value
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
                Some(current.saturating_add(amount))
            })
            .expect("counter update closure always returns a value");
        previous.saturating_add(amount)
    }

    /// Returns the current counter value.
    #[must_use]
    pub fn value(&self) -> u64 {
        self.value.load(Ordering::Relaxed)
    }

    /// Captures an owned counter snapshot.
    #[must_use]
    pub fn snapshot(&self) -> CounterSnapshot {
        CounterSnapshot {
            name: self.name.clone(),
            unit: self.unit,
            value: self.value(),
        }
    }

    /// Returns the stable counter name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns the counter unit.
    #[must_use]
    pub const fn unit(&self) -> CounterUnit {
        self.unit
    }
}

impl fmt::Debug for PerformanceCounter {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("PerformanceCounter")
            .field("name", &self.name)
            .field("unit", &self.unit)
            .field("value", &self.value())
            .finish()
    }
}

fn validate_diagnostic_name(kind: &'static str, value: &str) -> Result<()> {
    if is_canonical_name(value) {
        return Ok(());
    }

    Err(Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        format!("invalid diagnostic {kind} name"),
    )
    .with_context(
        ErrorContext::new("superi-core.diagnostics", "validate_name").with_field("kind", kind),
    ))
}

fn is_canonical_name(value: &str) -> bool {
    let mut bytes = value.bytes();
    let Some(first) = bytes.next() else {
        return false;
    };
    if !first.is_ascii_lowercase() {
        return false;
    }

    let mut previous_was_separator = false;
    for byte in bytes {
        if byte.is_ascii_lowercase() || byte.is_ascii_digit() {
            previous_was_separator = false;
        } else if matches!(byte, b'.' | b'_' | b'-') && !previous_was_separator {
            previous_was_separator = true;
        } else {
            return false;
        }
    }

    !previous_was_separator
}

fn invalid_diagnostic_value(kind: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        format!("invalid diagnostic {kind} value"),
    )
    .with_context(
        ErrorContext::new("superi-core.diagnostics", "validate_value").with_field("kind", kind),
    )
}
