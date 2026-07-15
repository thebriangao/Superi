//! Bounded deterministic cache-warming targets for likely edits and timeline scrubs.
//!
//! This module ranks exact timeline frame positions only. Callers map each target to their ordinary
//! graph evaluation request and complete cache scope, so warming cannot bypass source, graph,
//! parameter, color, render-setting, revision, budget, eviction, proxy, or fallback policy.

use std::collections::BTreeSet;

use superi_core::error::{Error, ErrorCategory, ErrorContext, Recoverability, Result};

const COMPONENT: &str = "superi-cache.warming";

/// One finite half-open timeline frame interval available for speculative warming.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CacheWarmBounds {
    start_frame: i64,
    end_frame_exclusive: i64,
}

impl CacheWarmBounds {
    /// Creates nonempty half-open frame bounds `[start_frame, end_frame_exclusive)`.
    pub fn new(start_frame: i64, end_frame_exclusive: i64) -> Result<Self> {
        if start_frame >= end_frame_exclusive {
            return Err(invalid_input(
                "cache warm bounds must contain at least one frame",
                "create_bounds",
                "empty_or_reversed_bounds",
            )
            .with_context(
                ErrorContext::new(COMPONENT, "create_bounds")
                    .with_field("start_frame", start_frame.to_string())
                    .with_field("end_frame_exclusive", end_frame_exclusive.to_string()),
            ));
        }
        Ok(Self {
            start_frame,
            end_frame_exclusive,
        })
    }

    /// Returns the first available timeline frame.
    #[must_use]
    pub const fn start_frame(self) -> i64 {
        self.start_frame
    }

    /// Returns the exclusive end of the available timeline frame interval.
    #[must_use]
    pub const fn end_frame_exclusive(self) -> i64 {
        self.end_frame_exclusive
    }

    /// Returns whether one exact frame lies inside the half-open interval.
    #[must_use]
    pub const fn contains(self, frame: i64) -> bool {
        frame >= self.start_frame && frame < self.end_frame_exclusive
    }

    const fn contains_boundary(self, frame: i64) -> bool {
        frame >= self.start_frame && frame <= self.end_frame_exclusive
    }
}

/// Hard limits and ranking horizons for one cache-warming planner.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub struct CacheWarmPolicy {
    edit_radius: u32,
    scrub_ahead: u32,
    scrub_behind: u32,
    maximum_scrub_stride: u32,
    maximum_targets: usize,
}

impl CacheWarmPolicy {
    /// Creates a checked bounded warming policy.
    ///
    /// `edit_radius` ranks frames on both sides of each edit boundary. `scrub_ahead` and
    /// `scrub_behind` count predicted observations in and against the observed scrub direction.
    /// The observed frame delta is capped by `maximum_scrub_stride`. Every plan is truncated at
    /// `maximum_targets`, which must be nonzero.
    pub fn new(
        edit_radius: u32,
        scrub_ahead: u32,
        scrub_behind: u32,
        maximum_scrub_stride: u32,
        maximum_targets: usize,
    ) -> Result<Self> {
        if maximum_scrub_stride == 0 {
            return Err(invalid_input(
                "cache warm scrub stride limit must be nonzero",
                "create_policy",
                "zero_scrub_stride",
            ));
        }
        if maximum_targets == 0 {
            return Err(invalid_input(
                "cache warm target limit must be nonzero",
                "create_policy",
                "zero_target_limit",
            ));
        }
        Ok(Self {
            edit_radius,
            scrub_ahead,
            scrub_behind,
            maximum_scrub_stride,
            maximum_targets,
        })
    }

    /// Returns the number of neighboring frames considered on each side of an edit boundary.
    #[must_use]
    pub const fn edit_radius(self) -> u32 {
        self.edit_radius
    }

    /// Returns the number of predicted positions considered in the observed scrub direction.
    #[must_use]
    pub const fn scrub_ahead(self) -> u32 {
        self.scrub_ahead
    }

    /// Returns the number of predicted positions considered against the observed scrub direction.
    #[must_use]
    pub const fn scrub_behind(self) -> u32 {
        self.scrub_behind
    }

    /// Returns the largest frame stride inferred from two scrub observations.
    #[must_use]
    pub const fn maximum_scrub_stride(self) -> u32 {
        self.maximum_scrub_stride
    }

    /// Returns the hard output limit for every plan.
    #[must_use]
    pub const fn maximum_targets(self) -> usize {
        self.maximum_targets
    }
}

/// Why one exact frame was ranked for speculative warming.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
#[non_exhaustive]
pub enum CacheWarmReason {
    /// The frame is at or near one canonical editorial edit boundary.
    EditBoundary,
    /// The frame follows an observed increasing timeline-frame scrub.
    ScrubForward,
    /// The frame follows an observed decreasing timeline-frame scrub.
    ScrubBackward,
    /// The frame is near a scrub observation that did not move.
    ScrubStationary,
}

/// One exact caller-evaluated timeline frame in deterministic warm priority order.
#[derive(Clone, Copy, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
pub struct CacheWarmTarget {
    frame: i64,
    reason: CacheWarmReason,
    rank: usize,
}

impl CacheWarmTarget {
    /// Returns the exact timeline frame the caller may evaluate through its ordinary cache scope.
    #[must_use]
    pub const fn frame(self) -> i64 {
        self.frame
    }

    /// Returns the observation class that produced this target.
    #[must_use]
    pub const fn reason(self) -> CacheWarmReason {
        self.reason
    }

    /// Returns the zero-based deterministic priority, where lower values warm first.
    #[must_use]
    pub const fn rank(self) -> usize {
        self.rank
    }
}

/// One immutable bounded cache-warming plan.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CacheWarmPlan {
    targets: Vec<CacheWarmTarget>,
}

impl CacheWarmPlan {
    /// Returns exact targets in deterministic priority order.
    #[must_use]
    pub fn targets(&self) -> &[CacheWarmTarget] {
        &self.targets
    }

    /// Returns the number of exact targets in this bounded plan.
    #[must_use]
    pub fn len(&self) -> usize {
        self.targets.len()
    }

    /// Returns whether no in-bounds target was produced.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.targets.is_empty()
    }
}

/// Stateless deterministic planner for likely edit and scrub cache targets.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct CacheWarmPlanner {
    policy: CacheWarmPolicy,
}

impl CacheWarmPlanner {
    /// Creates a planner from one already validated hard-limit policy.
    #[must_use]
    pub const fn new(policy: CacheWarmPolicy) -> Self {
        Self { policy }
    }

    /// Returns this planner's immutable hard-limit policy.
    #[must_use]
    pub const fn policy(self) -> CacheWarmPolicy {
        self.policy
    }

    /// Ranks frames nearest to canonical edit boundaries across the complete input set.
    ///
    /// Boundary order and duplicates do not change the plan. A boundary may equal the exclusive
    /// interval end so the last frame before an edit at timeline end can still be warmed. The
    /// number of distinct boundaries and output targets are both limited by `maximum_targets`.
    pub fn plan_edit<I>(&self, bounds: CacheWarmBounds, boundaries: I) -> Result<CacheWarmPlan>
    where
        I: IntoIterator<Item = i64>,
    {
        let mut canonical_boundaries = BTreeSet::new();
        for boundary in boundaries {
            if !bounds.contains_boundary(boundary) {
                return Err(position_error(
                    bounds,
                    boundary,
                    "plan_edit",
                    "edit_boundary_out_of_bounds",
                ));
            }
            canonical_boundaries.insert(boundary);
            if canonical_boundaries.len() > self.policy.maximum_targets {
                return Err(invalid_input(
                    "cache warm edit boundary count exceeds the plan target limit",
                    "plan_edit",
                    "too_many_edit_boundaries",
                ));
            }
        }

        let mut frames = Vec::new();
        let mut seen = BTreeSet::new();
        for distance in 0..=self.policy.edit_radius {
            for boundary in &canonical_boundaries {
                if distance == 0 {
                    push_frame(
                        &mut frames,
                        &mut seen,
                        bounds,
                        i128::from(*boundary),
                        self.policy.maximum_targets,
                    );
                } else {
                    let distance = i128::from(distance);
                    push_frame(
                        &mut frames,
                        &mut seen,
                        bounds,
                        i128::from(*boundary) - distance,
                        self.policy.maximum_targets,
                    );
                    push_frame(
                        &mut frames,
                        &mut seen,
                        bounds,
                        i128::from(*boundary) + distance,
                        self.policy.maximum_targets,
                    );
                }
                if frames.len() == self.policy.maximum_targets {
                    return Ok(finish_plan(frames, CacheWarmReason::EditBoundary));
                }
            }
        }
        Ok(finish_plan(frames, CacheWarmReason::EditBoundary))
    }

    /// Ranks the current scrub frame, predicted motion, and a smaller opposite-direction tail.
    ///
    /// Both observations must lie inside the available interval. Nonzero motion uses the exact
    /// observed delta capped by `maximum_scrub_stride`. Stationary observations alternate nearest
    /// earlier and later frames. Every candidate remains exact and clipped to `bounds`.
    pub fn plan_scrub(
        &self,
        bounds: CacheWarmBounds,
        previous_frame: i64,
        current_frame: i64,
    ) -> Result<CacheWarmPlan> {
        if !bounds.contains(previous_frame) {
            return Err(position_error(
                bounds,
                previous_frame,
                "plan_scrub",
                "previous_scrub_frame_out_of_bounds",
            ));
        }
        if !bounds.contains(current_frame) {
            return Err(position_error(
                bounds,
                current_frame,
                "plan_scrub",
                "current_scrub_frame_out_of_bounds",
            ));
        }

        let delta = i128::from(current_frame) - i128::from(previous_frame);
        if delta == 0 {
            return Ok(self.plan_stationary_scrub(bounds, current_frame));
        }

        let reason = if delta > 0 {
            CacheWarmReason::ScrubForward
        } else {
            CacheWarmReason::ScrubBackward
        };
        let direction = delta.signum();
        let stride = delta
            .abs()
            .min(i128::from(self.policy.maximum_scrub_stride));
        let current = i128::from(current_frame);
        let mut frames = Vec::new();
        let mut seen = BTreeSet::new();
        push_frame(
            &mut frames,
            &mut seen,
            bounds,
            current,
            self.policy.maximum_targets,
        );

        for step in 1..=self.policy.scrub_ahead {
            let candidate = current + direction * stride * i128::from(step);
            if !push_frame(
                &mut frames,
                &mut seen,
                bounds,
                candidate,
                self.policy.maximum_targets,
            ) {
                break;
            }
            if frames.len() == self.policy.maximum_targets {
                return Ok(finish_plan(frames, reason));
            }
        }
        for step in 1..=self.policy.scrub_behind {
            let candidate = current - direction * stride * i128::from(step);
            if !push_frame(
                &mut frames,
                &mut seen,
                bounds,
                candidate,
                self.policy.maximum_targets,
            ) {
                break;
            }
            if frames.len() == self.policy.maximum_targets {
                break;
            }
        }
        Ok(finish_plan(frames, reason))
    }

    fn plan_stationary_scrub(&self, bounds: CacheWarmBounds, current_frame: i64) -> CacheWarmPlan {
        let mut frames = Vec::new();
        let mut seen = BTreeSet::new();
        let current = i128::from(current_frame);
        push_frame(
            &mut frames,
            &mut seen,
            bounds,
            current,
            self.policy.maximum_targets,
        );
        let neighbor_limit =
            u64::from(self.policy.scrub_ahead).saturating_add(u64::from(self.policy.scrub_behind));
        let neighbor_limit = usize::try_from(neighbor_limit)
            .unwrap_or(usize::MAX)
            .min(self.policy.maximum_targets.saturating_sub(1));
        let mut neighbors = 0;
        let mut distance = 1_i128;

        while neighbors < neighbor_limit && frames.len() < self.policy.maximum_targets {
            let before = current - distance;
            let after = current + distance;
            if push_frame(
                &mut frames,
                &mut seen,
                bounds,
                before,
                self.policy.maximum_targets,
            ) {
                neighbors += 1;
            }
            if neighbors < neighbor_limit
                && push_frame(
                    &mut frames,
                    &mut seen,
                    bounds,
                    after,
                    self.policy.maximum_targets,
                )
            {
                neighbors += 1;
            }
            if before < i128::from(bounds.start_frame)
                && after >= i128::from(bounds.end_frame_exclusive)
            {
                break;
            }
            distance += 1;
        }
        finish_plan(frames, CacheWarmReason::ScrubStationary)
    }
}

fn finish_plan(frames: Vec<i64>, reason: CacheWarmReason) -> CacheWarmPlan {
    CacheWarmPlan {
        targets: frames
            .into_iter()
            .enumerate()
            .map(|(rank, frame)| CacheWarmTarget {
                frame,
                reason,
                rank,
            })
            .collect(),
    }
}

fn push_frame(
    frames: &mut Vec<i64>,
    seen: &mut BTreeSet<i64>,
    bounds: CacheWarmBounds,
    candidate: i128,
    maximum_targets: usize,
) -> bool {
    if frames.len() == maximum_targets
        || candidate < i128::from(bounds.start_frame)
        || candidate >= i128::from(bounds.end_frame_exclusive)
    {
        return false;
    }
    let frame = i64::try_from(candidate).expect("in-bounds cache warm frame fits i64");
    if seen.insert(frame) {
        frames.push(frame);
        true
    } else {
        false
    }
}

fn position_error(
    bounds: CacheWarmBounds,
    frame: i64,
    operation: &'static str,
    reason: &'static str,
) -> Error {
    invalid_input(
        "cache warm position lies outside the available timeline bounds",
        operation,
        reason,
    )
    .with_context(
        ErrorContext::new(COMPONENT, operation)
            .with_field("frame", frame.to_string())
            .with_field("start_frame", bounds.start_frame.to_string())
            .with_field(
                "end_frame_exclusive",
                bounds.end_frame_exclusive.to_string(),
            ),
    )
}

fn invalid_input(message: &'static str, operation: &'static str, reason: &'static str) -> Error {
    Error::new(
        ErrorCategory::InvalidInput,
        Recoverability::UserCorrectable,
        message,
    )
    .with_context(ErrorContext::new(COMPONENT, operation).with_field("reason", reason))
}
