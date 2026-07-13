//! Bounded, nonblocking handoffs between media pipeline owners.
//!
//! Each handoff has one directed [`PipelineRoute`] and its own fixed capacity. Queue storage is
//! allocated when [`bounded_handoff`] is called. Sending and receiving never wait for capacity,
//! take a blocking lock, or allocate queue storage. When a handoff is saturated, [`Backpressure`]
//! returns the exact item to its producer instead of dropping, replacing, or copying it.
//!
//! Lossless audio and export owners retain the returned item and reschedule production. Viewport
//! owners pass a returned video frame to the A/V scheduler when an intentional frame-drop decision
//! may be appropriate. This module does not invent a second priority policy, frame-drop policy,
//! lifecycle protocol, or payload representation.

use std::fmt;
use std::sync::Arc;

use crossbeam_queue::ArrayQueue;
use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

const COMPONENT: &str = "superi-concurrency.backpressure";

/// A semantic owner in the bounded media pipeline.
///
/// These values name ownership boundaries, not Rust crate dependencies. The concurrency crate stays
/// below media I/O, graph, cache, audio, and engine orchestration in the dependency graph.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum PipelineStage {
    /// Codec-neutral decode work and decoded output ownership.
    Decode,
    /// Lazy graph evaluation and render coordination.
    Graph,
    /// Final-frame, intermediate, proxy, or render-cache ownership.
    Cache,
    /// Audio staging before the platform-owned real-time callback.
    Audio,
    /// Interactive native viewport presentation staging.
    Viewport,
    /// Lossless render and export delivery work.
    Export,
}

impl PipelineStage {
    /// Every pipeline stage in stable diagnostic order.
    pub const ALL: &'static [Self] = &[
        Self::Decode,
        Self::Graph,
        Self::Cache,
        Self::Audio,
        Self::Viewport,
        Self::Export,
    ];

    /// Returns the stable diagnostic code for this stage.
    #[must_use]
    pub const fn code(self) -> &'static str {
        match self {
            Self::Decode => "decode",
            Self::Graph => "graph",
            Self::Cache => "cache",
            Self::Audio => "audio",
            Self::Viewport => "viewport",
            Self::Export => "export",
        }
    }

    /// Looks up a stage from its stable diagnostic code.
    #[must_use]
    pub fn from_code(code: &str) -> Option<Self> {
        match code {
            "decode" => Some(Self::Decode),
            "graph" => Some(Self::Graph),
            "cache" => Some(Self::Cache),
            "audio" => Some(Self::Audio),
            "viewport" => Some(Self::Viewport),
            "export" => Some(Self::Export),
            _ => None,
        }
    }
}

impl fmt::Display for PipelineStage {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.code())
    }
}

/// One directed ownership boundary in a media pipeline.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct PipelineRoute {
    producer: PipelineStage,
    consumer: PipelineStage,
}

impl PipelineRoute {
    /// Creates a directed route between distinct stage owners.
    pub fn new(producer: PipelineStage, consumer: PipelineStage) -> Result<Self> {
        if producer == consumer {
            return Err(Error::new(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "a backpressure route requires distinct producer and consumer stages",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "create_route")
                    .with_field("producer", producer.code())
                    .with_field("consumer", consumer.code()),
            ));
        }
        Ok(Self { producer, consumer })
    }

    /// Returns the upstream owner.
    #[must_use]
    pub const fn producer(self) -> PipelineStage {
        self.producer
    }

    /// Returns the downstream owner.
    #[must_use]
    pub const fn consumer(self) -> PipelineStage {
        self.consumer
    }
}

impl fmt::Display for PipelineRoute {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{} -> {}", self.producer, self.consumer)
    }
}

/// Immutable configuration for one independently bounded handoff.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct BackpressureConfig {
    route: PipelineRoute,
    capacity: usize,
}

impl BackpressureConfig {
    /// Creates a handoff configuration with a hard positive item bound.
    pub fn new(route: PipelineRoute, capacity: usize) -> Result<Self> {
        if capacity == 0 {
            return Err(Error::new(
                ErrorCategory::InvalidInput,
                Recoverability::UserCorrectable,
                "backpressure capacity must be greater than zero",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "configure")
                    .with_field("producer", route.producer.code())
                    .with_field("consumer", route.consumer.code())
                    .with_field("capacity", capacity.to_string()),
            ));
        }
        Ok(Self { route, capacity })
    }

    /// Returns the configured ownership route.
    #[must_use]
    pub const fn route(self) -> PipelineRoute {
        self.route
    }

    /// Returns the maximum number of queued items.
    #[must_use]
    pub const fn capacity(self) -> usize {
        self.capacity
    }
}

/// One point-in-time observation of a handoff's bounded occupancy.
///
/// Another producer or consumer may change occupancy immediately after this value is returned.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct HandoffSnapshot {
    route: PipelineRoute,
    capacity: usize,
    queued_items: usize,
}

impl HandoffSnapshot {
    /// Returns the directed ownership route.
    #[must_use]
    pub const fn route(self) -> PipelineRoute {
        self.route
    }

    /// Returns the hard queue bound.
    #[must_use]
    pub const fn capacity(self) -> usize {
        self.capacity
    }

    /// Returns the observed number of queued items.
    #[must_use]
    pub const fn queued_items(self) -> usize {
        self.queued_items
    }

    /// Returns the observed capacity available to producers.
    #[must_use]
    pub const fn remaining_capacity(self) -> usize {
        self.capacity.saturating_sub(self.queued_items)
    }

    /// Returns whether the handoff was observed empty.
    #[must_use]
    pub const fn is_empty(self) -> bool {
        self.queued_items == 0
    }

    /// Returns whether the handoff was observed at its hard bound.
    #[must_use]
    pub const fn is_full(self) -> bool {
        self.queued_items == self.capacity
    }
}

/// A normal flow-control signal that retains ownership of a saturated payload.
pub struct Backpressure<T> {
    route: PipelineRoute,
    capacity: usize,
    item: T,
}

impl<T> Backpressure<T> {
    /// Returns the stable saturation code.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        "capacity_reached"
    }

    /// Returns the saturated route.
    #[must_use]
    pub const fn route(&self) -> PipelineRoute {
        self.route
    }

    /// Returns the route's hard item bound.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.capacity
    }

    /// Borrows the item retained by the producer.
    #[must_use]
    pub const fn item(&self) -> &T {
        &self.item
    }

    /// Recovers the item for deterministic retry, rescheduling, or an explicit drop decision.
    #[must_use]
    pub fn into_item(self) -> T {
        self.item
    }
}

impl<T> fmt::Debug for Backpressure<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Backpressure")
            .field("route", &self.route)
            .field("capacity", &self.capacity)
            .field("item_type", &std::any::type_name::<T>())
            .finish()
    }
}

impl<T> fmt::Display for Backpressure<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "{} reached its configured capacity of {} items",
            self.route, self.capacity
        )
    }
}

struct SharedHandoff<T> {
    config: BackpressureConfig,
    queue: ArrayQueue<T>,
}

impl<T> SharedHandoff<T> {
    fn snapshot(&self) -> HandoffSnapshot {
        HandoffSnapshot {
            route: self.config.route,
            capacity: self.config.capacity,
            queued_items: self.queue.len(),
        }
    }
}

/// Cloneable producer endpoint for one bounded pipeline route.
pub struct HandoffSender<T> {
    shared: Arc<SharedHandoff<T>>,
}

impl<T> HandoffSender<T> {
    /// Attempts to enqueue without waiting or allocating queue storage.
    ///
    /// Saturation returns [`Backpressure`] with the exact item still owned by the producer.
    pub fn try_send(&self, item: T) -> std::result::Result<(), Backpressure<T>> {
        self.shared.queue.push(item).map_err(|item| Backpressure {
            route: self.shared.config.route,
            capacity: self.shared.config.capacity,
            item,
        })
    }

    /// Returns a point-in-time occupancy observation.
    #[must_use]
    pub fn snapshot(&self) -> HandoffSnapshot {
        self.shared.snapshot()
    }
}

impl<T> Clone for HandoffSender<T> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<T> fmt::Debug for HandoffSender<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HandoffSender")
            .field("snapshot", &self.snapshot())
            .finish_non_exhaustive()
    }
}

/// Cloneable consumer endpoint for one bounded pipeline route.
pub struct HandoffReceiver<T> {
    shared: Arc<SharedHandoff<T>>,
}

impl<T> HandoffReceiver<T> {
    /// Receives one queued item without waiting or allocating queue storage.
    #[must_use]
    pub fn try_receive(&self) -> Option<T> {
        self.shared.queue.pop()
    }

    /// Returns a point-in-time occupancy observation.
    #[must_use]
    pub fn snapshot(&self) -> HandoffSnapshot {
        self.shared.snapshot()
    }
}

impl<T> Clone for HandoffReceiver<T> {
    fn clone(&self) -> Self {
        Self {
            shared: Arc::clone(&self.shared),
        }
    }
}

impl<T> fmt::Debug for HandoffReceiver<T> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HandoffReceiver")
            .field("snapshot", &self.snapshot())
            .finish_non_exhaustive()
    }
}

/// Creates typed producer and consumer endpoints over one fixed-capacity queue.
///
/// Endpoints may be cloned for multiple producers or consumers. Successfully enqueued items are
/// received in queue order. The pipeline's own payload continues to carry exact timing, metadata,
/// color, alpha, channel, generation, and storage ownership information without interpretation or
/// copying by this layer.
#[must_use]
pub fn bounded_handoff<T>(config: BackpressureConfig) -> (HandoffSender<T>, HandoffReceiver<T>) {
    let shared = Arc::new(SharedHandoff {
        config,
        queue: ArrayQueue::new(config.capacity),
    });
    (
        HandoffSender {
            shared: Arc::clone(&shared),
        },
        HandoffReceiver { shared },
    )
}
