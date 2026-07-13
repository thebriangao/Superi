//! Safe ownership contracts for mutable state and immutable cross-thread snapshots.
//!
//! [`DomainOwned`] keeps mutable state behind one explicit [`ExecutionDomain`]. The owner can move
//! between threads when its payload is `Send`, but it is intentionally not `Sync`, so an `Arc` or
//! shared reference cannot turn it into concurrently mutable state. Access checks the active domain
//! and never acquires a lock.
//!
//! [`SnapshotPublisher`] is another single-owner value. It publishes immutable, monotonically tagged
//! [`SharedSnapshot`] values from an existing [`Arc`]. Readers can clone a snapshot without copying
//! the payload, holding a lock, or delaying its owner. Reference-counted snapshots are not an audio
//! callback primitive because their final drop can free memory. Audio continues to use preallocated
//! atomics and buffers.

use std::cell::Cell;
use std::fmt;
use std::marker::PhantomData;
use std::ops::Deref;
use std::sync::Arc;

use crate::threads::ExecutionDomain;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result, ResultExt};

const COMPONENT: &str = "superi-concurrency.shared";

/// Mutable state held by exactly one execution-domain owner.
///
/// `DomainOwned<T>` is `Send` when `T` is `Send`, which allows an explicit ownership transfer into a
/// managed domain thread. It is never `Sync`, so it cannot be placed behind shared ownership and
/// accessed concurrently. The wrapper uses only safe Rust auto traits and contains no lock.
///
/// ```compile_fail
/// use superi_concurrency::shared::DomainOwned;
/// use superi_concurrency::threads::ExecutionDomain;
///
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<DomainOwned<String>>();
/// let _ = ExecutionDomain::EngineControl;
/// ```
pub struct DomainOwned<T> {
    domain: ExecutionDomain,
    value: T,
    single_owner: PhantomData<Cell<()>>,
}

impl<T> DomainOwned<T> {
    /// Creates state assigned to one execution domain.
    ///
    /// Construction does not claim the current thread. This allows setup code to build a value and
    /// move it into the managed owner before access begins.
    #[must_use]
    pub const fn new(domain: ExecutionDomain, value: T) -> Self {
        Self {
            domain,
            value,
            single_owner: PhantomData,
        }
    }

    /// Returns the domain that owns this state.
    #[must_use]
    pub const fn domain(&self) -> ExecutionDomain {
        self.domain
    }

    /// Runs one immutable operation after verifying the current execution domain.
    pub fn with<R>(&self, operation: impl FnOnce(&T) -> R) -> Result<R> {
        require_owner(self.domain, "read_owned_state")?;
        Ok(operation(&self.value))
    }

    /// Runs one exclusive operation after verifying the current execution domain.
    ///
    /// The callback executes inline and must remain bounded for UI, engine-control, playback, and
    /// audio owners according to [`ExecutionDomain::policy`].
    pub fn with_mut<R>(&mut self, operation: impl FnOnce(&mut T) -> R) -> Result<R> {
        require_owner(self.domain, "mutate_owned_state")?;
        Ok(operation(&mut self.value))
    }

    /// Consumes the ownership wrapper after verifying the current execution domain.
    pub fn into_inner(self) -> Result<T> {
        require_owner(self.domain, "release_owned_state")?;
        Ok(self.value)
    }
}

impl<T> fmt::Debug for DomainOwned<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("DomainOwned")
            .field("domain", &self.domain)
            .finish_non_exhaustive()
    }
}

/// Monotonic identity for one publisher's immutable snapshot stream.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct SnapshotGeneration(u64);

impl SnapshotGeneration {
    /// State before a publisher has emitted its first snapshot.
    pub const INITIAL: Self = Self(0);

    /// Creates a generation from its stable integer value.
    #[must_use]
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    /// Returns the stable integer value.
    #[must_use]
    pub const fn get(self) -> u64 {
        self.0
    }

    fn next(self, domain: ExecutionDomain) -> Result<Self> {
        self.0.checked_add(1).map(Self).ok_or_else(|| {
            Error::new(
                ErrorCategory::ResourceExhausted,
                Recoverability::Terminal,
                "shared snapshot generation is exhausted",
            )
            .with_context(
                shared_context("publish_snapshot", domain)
                    .with_field("generation", self.0.to_string()),
            )
        })
    }
}

impl fmt::Display for SnapshotGeneration {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Single-owner publisher for one immutable snapshot stream.
///
/// The publisher is `Send` when `T` is `Send`, but it is never `Sync`. Publication requires its
/// configured execution domain, rejects the allocation-free audio callback, and advances the
/// generation only after every precondition succeeds.
///
/// ```compile_fail
/// use superi_concurrency::shared::SnapshotPublisher;
///
/// fn assert_sync<T: Sync>() {}
/// assert_sync::<SnapshotPublisher<String>>();
/// ```
pub struct SnapshotPublisher<T> {
    domain: ExecutionDomain,
    generation: SnapshotGeneration,
    single_owner: PhantomData<Cell<T>>,
}

impl<T> SnapshotPublisher<T> {
    /// Creates a publisher before its first snapshot.
    #[must_use]
    pub const fn new(domain: ExecutionDomain) -> Self {
        Self::from_generation(domain, SnapshotGeneration::INITIAL)
    }

    /// Restores a publisher after an already committed generation.
    ///
    /// The next successful publication advances from `generation`. This constructor supports
    /// lifecycle restoration without reusing an identity.
    #[must_use]
    pub const fn from_generation(domain: ExecutionDomain, generation: SnapshotGeneration) -> Self {
        Self {
            domain,
            generation,
            single_owner: PhantomData,
        }
    }

    /// Returns the domain that owns this publisher.
    #[must_use]
    pub const fn domain(&self) -> ExecutionDomain {
        self.domain
    }

    /// Returns the most recently committed generation.
    #[must_use]
    pub const fn current_generation(&self) -> SnapshotGeneration {
        self.generation
    }

    /// Publishes an immutable shared snapshot without cloning its payload.
    ///
    /// The caller retains its `Arc`, so failed publication does not consume or hide the prepared
    /// state. Successful publication clones only the atomic ownership handle.
    pub fn publish(&mut self, value: &Arc<T>) -> Result<SharedSnapshot<T>>
    where
        T: Send + Sync,
    {
        require_owner(self.domain, "publish_snapshot")?;
        if !self.domain.policy().allows_allocation() {
            return Err(Error::new(
                ErrorCategory::Conflict,
                Recoverability::UserCorrectable,
                "reference-counted snapshots cannot be published from an allocation-free domain",
            )
            .with_context(shared_context("publish_snapshot", self.domain)));
        }

        let generation = self.generation.next(self.domain)?;
        let snapshot = SharedSnapshot {
            source_domain: self.domain,
            generation,
            value: Arc::clone(value),
        };
        self.generation = generation;
        Ok(snapshot)
    }
}

impl<T> fmt::Debug for SnapshotPublisher<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SnapshotPublisher")
            .field("domain", &self.domain)
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

/// Immutable generated state safe to inspect across thread owners.
///
/// `SharedSnapshot<T>` is `Send` and `Sync` exactly when `T` is both `Send` and `Sync`. Cloning a
/// snapshot clones only its [`Arc`] owner and retains the source domain and generation.
pub struct SharedSnapshot<T> {
    source_domain: ExecutionDomain,
    generation: SnapshotGeneration,
    value: Arc<T>,
}

impl<T> SharedSnapshot<T> {
    /// Returns the domain that published this snapshot.
    #[must_use]
    pub const fn source_domain(&self) -> ExecutionDomain {
        self.source_domain
    }

    /// Returns this snapshot's monotonic generation.
    #[must_use]
    pub const fn generation(&self) -> SnapshotGeneration {
        self.generation
    }

    /// Borrows the immutable snapshot payload.
    #[must_use]
    pub fn value(&self) -> &T {
        &self.value
    }

    /// Returns whether two snapshots retain the same immutable allocation.
    #[must_use]
    pub fn ptr_eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.value, &other.value)
    }
}

impl<T> Clone for SharedSnapshot<T> {
    fn clone(&self) -> Self {
        Self {
            source_domain: self.source_domain,
            generation: self.generation,
            value: Arc::clone(&self.value),
        }
    }
}

impl<T> Deref for SharedSnapshot<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        self.value()
    }
}

impl<T> fmt::Debug for SharedSnapshot<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("SharedSnapshot")
            .field("source_domain", &self.source_domain)
            .field("generation", &self.generation)
            .finish_non_exhaustive()
    }
}

fn require_owner(domain: ExecutionDomain, operation: &'static str) -> Result<()> {
    domain
        .require_current()
        .with_error_context(shared_context(operation, domain))
}

fn shared_context(operation: &'static str, domain: ExecutionDomain) -> ErrorContext {
    ErrorContext::new(COMPONENT, operation).with_field("owner_domain", domain.code())
}
