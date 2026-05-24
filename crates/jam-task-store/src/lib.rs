//! SQLite-backed event store for Jamboree task aggregates.
//!
//! Provides durable storage with:
//! - **Event sourcing**: task state is derived from an append-only event stream
//! - **Optimistic concurrency**: version checks prevent concurrent corruption
//! - **Idempotency tracking**: duplicate events are detected and rejected
//! - **Materialized projections**: a `task_state` table for fast queries
//! - **Snapshots**: optional aggregate snapshots for faster rebuilds

#![deny(missing_docs)]

mod query;
mod schema;
mod store;

pub use query::{TaskFilter, TaskSummary};
pub use store::{AppendError, TaskStore};
