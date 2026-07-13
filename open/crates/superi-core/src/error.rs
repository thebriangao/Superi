//! Shared error vocabulary and actionable diagnostic context.
//!
//! Error categories and recoverability values have stable string codes so every
//! engine, project, extension, and automation boundary can interpret them in the
//! same way. Transport-specific numeric codes and serialization belong to their
//! respective API and serialization layers.

use std::collections::BTreeMap;
use std::error::Error as StdError;
use std::fmt;

/// A stable, domain-independent category for a failure.
///
/// The variant names are the Rust API. [`ErrorCategory::code`] is the permanent
/// cross-process identifier. Consumers must include a wildcard arm when matching
/// so later versions can add categories without breaking existing clients.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum ErrorCategory {
    /// An input or argument is invalid.
    InvalidInput,
    /// A requested resource does not exist.
    NotFound,
    /// Current state conflicts with the requested operation.
    Conflict,
    /// The requested operation or format is not supported.
    Unsupported,
    /// The caller lacks permission for the requested operation.
    PermissionDenied,
    /// A bounded resource cannot satisfy the request.
    ResourceExhausted,
    /// A required resource or subsystem is currently unavailable.
    Unavailable,
    /// The operation exceeded its allowed time.
    Timeout,
    /// The operation was cancelled before completion.
    Cancelled,
    /// Input or stored data is corrupt or inconsistent.
    CorruptData,
    /// An invariant failed or an unclassified implementation defect occurred.
    Internal,
}

impl ErrorCategory {
    /// Every category defined by this version, in stable code order.
    pub const ALL: &'static [Self] = &[
        Self::InvalidInput,
        Self::NotFound,
        Self::Conflict,
        Self::Unsupported,
        Self::PermissionDenied,
        Self::ResourceExhausted,
        Self::Unavailable,
        Self::Timeout,
        Self::Cancelled,
        Self::CorruptData,
        Self::Internal,
    ];

    /// Returns the permanent cross-process code for this category.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::InvalidInput => "invalid_input",
            Self::NotFound => "not_found",
            Self::Conflict => "conflict",
            Self::Unsupported => "unsupported",
            Self::PermissionDenied => "permission_denied",
            Self::ResourceExhausted => "resource_exhausted",
            Self::Unavailable => "unavailable",
            Self::Timeout => "timeout",
            Self::Cancelled => "cancelled",
            Self::CorruptData => "corrupt_data",
            Self::Internal => "internal",
        }
    }

    /// Looks up a category by its permanent code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "invalid_input" => Some(Self::InvalidInput),
            "not_found" => Some(Self::NotFound),
            "conflict" => Some(Self::Conflict),
            "unsupported" => Some(Self::Unsupported),
            "permission_denied" => Some(Self::PermissionDenied),
            "resource_exhausted" => Some(Self::ResourceExhausted),
            "unavailable" => Some(Self::Unavailable),
            "timeout" => Some(Self::Timeout),
            "cancelled" => Some(Self::Cancelled),
            "corrupt_data" => Some(Self::CorruptData),
            "internal" => Some(Self::Internal),
            _ => None,
        }
    }
}

impl fmt::Display for ErrorCategory {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// The action a consumer may take after a failure.
///
/// Classification is explicit for each error instance. A category alone cannot
/// determine recovery because the same kind of failure can have different
/// consequences in interactive, background, and shutdown operations.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
#[non_exhaustive]
pub enum Recoverability {
    /// Retrying the same operation may succeed without user intervention.
    Retryable,
    /// The requested result is unavailable, but the broader workflow may continue.
    Degraded,
    /// The user can correct input, permissions, configuration, or project state.
    UserCorrectable,
    /// The operation cannot continue safely in the current lifetime.
    Terminal,
}

impl Recoverability {
    /// Every classification defined by this version, in stable code order.
    pub const ALL: &'static [Self] = &[
        Self::Retryable,
        Self::Degraded,
        Self::UserCorrectable,
        Self::Terminal,
    ];

    /// Returns the permanent cross-process code for this classification.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Retryable => "retryable",
            Self::Degraded => "degraded",
            Self::UserCorrectable => "user_correctable",
            Self::Terminal => "terminal",
        }
    }

    /// Looks up a classification by its permanent code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "retryable" => Some(Self::Retryable),
            "degraded" => Some(Self::Degraded),
            "user_correctable" => Some(Self::UserCorrectable),
            "terminal" => Some(Self::Terminal),
            _ => None,
        }
    }
}

impl fmt::Display for Recoverability {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// One owned frame of actionable failure context.
///
/// Fields use sorted keys for deterministic diagnostics and later transport
/// projection. Inserting a duplicate key replaces its previous value.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ErrorContext {
    component: String,
    operation: String,
    fields: BTreeMap<String, String>,
}

impl ErrorContext {
    /// Creates a context frame for a component operation.
    pub fn new(component: impl Into<String>, operation: impl Into<String>) -> Self {
        Self {
            component: component.into(),
            operation: operation.into(),
            fields: BTreeMap::new(),
        }
    }

    /// Adds a field and returns the updated frame.
    #[must_use]
    pub fn with_field(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.insert_field(key, value);
        self
    }

    /// Inserts a field, returning the previous value when the key already existed.
    pub fn insert_field(
        &mut self,
        key: impl Into<String>,
        value: impl Into<String>,
    ) -> Option<String> {
        self.fields.insert(key.into(), value.into())
    }

    /// Returns the component that owns this context frame.
    #[must_use]
    pub fn component(&self) -> &str {
        &self.component
    }

    /// Returns the operation active when this frame was added.
    #[must_use]
    pub fn operation(&self) -> &str {
        &self.operation
    }

    /// Returns one field value by key.
    #[must_use]
    pub fn field(&self, key: &str) -> Option<&str> {
        self.fields.get(key).map(String::as_str)
    }

    /// Returns all fields in deterministic key order.
    #[must_use]
    pub fn fields(&self) -> &BTreeMap<String, String> {
        &self.fields
    }
}

impl fmt::Display for ErrorContext {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}.{}", self.component, self.operation)?;
        if !self.fields.is_empty() {
            formatter.write_str("(")?;
            for (index, (key, value)) in self.fields.iter().enumerate() {
                if index > 0 {
                    formatter.write_str(", ")?;
                }
                write!(formatter, "{key}={value}")?;
            }
            formatter.write_str(")")?;
        }
        Ok(())
    }
}

/// The shared runtime error used across Superi subsystems.
///
/// The summary and context are diagnostic values. They are not automatically
/// safe for user presentation because context may include paths, identifiers,
/// or implementation details. User-safe rendering belongs to the diagnostics
/// layer.
#[derive(Debug)]
pub struct Error {
    category: ErrorCategory,
    recoverability: Recoverability,
    message: String,
    contexts: Vec<ErrorContext>,
    source: Option<Box<dyn StdError + Send + Sync + 'static>>,
}

impl Error {
    /// Creates a classified error without a lower-level source.
    pub fn new(
        category: ErrorCategory,
        recoverability: Recoverability,
        message: impl Into<String>,
    ) -> Self {
        Self {
            category,
            recoverability,
            message: message.into(),
            contexts: Vec::new(),
            source: None,
        }
    }

    /// Creates a classified error that retains its lower-level source.
    pub fn with_source<E>(
        category: ErrorCategory,
        recoverability: Recoverability,
        message: impl Into<String>,
        source: E,
    ) -> Self
    where
        E: StdError + Send + Sync + 'static,
    {
        Self {
            category,
            recoverability,
            message: message.into(),
            contexts: Vec::new(),
            source: Some(Box::new(source)),
        }
    }

    /// Adds a local context frame and returns the updated error.
    ///
    /// Frames are stored from the failing operation toward its outer callers.
    #[must_use]
    pub fn with_context(mut self, context: ErrorContext) -> Self {
        self.push_context(context);
        self
    }

    /// Adds a local context frame in place.
    pub fn push_context(&mut self, context: ErrorContext) {
        self.contexts.push(context);
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

    /// Returns the concise diagnostic summary.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Returns context frames from the failing operation toward outer callers.
    #[must_use]
    pub fn contexts(&self) -> &[ErrorContext] {
        &self.contexts
    }
}

impl fmt::Display for Error {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)?;
        if !self.contexts.is_empty() {
            formatter.write_str(" [")?;
            for (index, context) in self.contexts.iter().rev().enumerate() {
                if index > 0 {
                    formatter.write_str(" -> ")?;
                }
                write!(formatter, "{context}")?;
            }
            formatter.write_str("]")?;
        }
        Ok(())
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        self.source
            .as_ref()
            .map(|source| source.as_ref() as &(dyn StdError + 'static))
    }
}

/// The canonical result type for shared Superi failures.
pub type Result<T> = std::result::Result<T, Error>;

/// Adds actionable context while preserving an existing shared error.
pub trait ResultExt<T> {
    /// Adds a context frame without changing classification, summary, or source.
    fn with_error_context(self, context: ErrorContext) -> Result<T>;
}

impl<T> ResultExt<T> for Result<T> {
    fn with_error_context(self, context: ErrorContext) -> Result<T> {
        self.map_err(|mut error| {
            error.push_context(context);
            error
        })
    }
}
