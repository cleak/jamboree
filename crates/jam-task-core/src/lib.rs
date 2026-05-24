//! Pure domain model for the Jamboree task lifecycle.
//!
//! Zero IO dependencies — no NATS, no filesystem, no async runtime. This crate
//! defines the canonical state machine, domain events, and aggregate for tasks.
//! All task state is derived by replaying events through the aggregate.
//!
//! Consumers (e.g. `jam-task-store`, `jam-task-lifecycle`) translate external
//! signals (NATS journal events) into domain events and feed them here.

#![deny(missing_docs)]

mod aggregate;
mod error;
mod event;
mod status;

pub use aggregate::{PrInfo, TaskAggregate};
pub use error::{ApplyError, ApplyOutcome};
pub use event::{ContinuationPhase, TaskEvent};
pub use status::{Priority, TaskStatus};

/// Per-task iteration cap for post-picker continuations.
///
/// Each round of post-picker coordination (bad pre-checks, CI failures,
/// CodeRabbit-requested changes) counts as one attempt. Pre-PR and post-PR
/// continuations are tracked independently: burning cap budget on picker-quality
/// issues before the PR opens leaves budget for the CI/review cycle.
///
/// 5 is calibrated for the CodeRabbit loop: initial review + address + follow-up
/// review + approve, with room for two more cycles.
pub const CONTINUATION_CAP: u32 = 5;
