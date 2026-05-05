//! Event manifest and shared envelope for the Jamboree orchestrator's NATS bus.
//!
//! Per `dec-events-toml-manifest` and spec §4.4.3: [`events.toml`] is the
//! single source of truth for event shapes. Per-event Rust types are generated
//! by `tools/events-codegen.py` (run as Cargo build script + pre-commit hook).
//!
//! For Phase 0, this crate provides only the shared [`EventEnvelope`].
//! Per-event payload types land in `src/generated/types.rs` once codegen is
//! wired up (`task-events-codegen-pipeline`).
//!
//! [`events.toml`]: https://github.com/cleak/jamboree/blob/main/crates/jam-events/events.toml

#![deny(missing_docs)]

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Current envelope schema version. Bumped only on breaking changes to the
/// envelope itself; per-event versioning is in `event_subtype_version`.
pub const ENVELOPE_SCHEMA_VERSION: u32 = 1;

/// JSONL envelope for every orchestrator journal entry.
///
/// Per spec §4.4.2 and §23.3.5: `trace_id` and `parent_trace_id` are at the
/// top level (not buried in `payload`) so trace queries are O(1) per-day-file
/// without payload parsing.
///
/// Field order matches the spec's example envelope:
///
/// ```jsonl
/// {"schema_version":1,"event_type":"picker.spawned","event_subtype_version":1,
///  "timestamp":"2026-05-02T15:32:18.123456789Z","journal_seq":48291,
///  "trace_id":"01HXKJ...","parent_trace_id":"01HXKH...",
///  "actor":"jam-svc-session","payload":{...}}
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventEnvelope<P> {
    /// Envelope schema version. See [`ENVELOPE_SCHEMA_VERSION`].
    pub schema_version: u32,

    /// Kebab-case dotted event type (e.g. `"picker.spawned"`, `"pr.merged"`).
    pub event_type: String,

    /// Per-event-type version. Bumps on additive changes; breaking changes
    /// get new event types entirely (e.g. `picker.spawned.v2`).
    pub event_subtype_version: u32,

    /// UTC RFC 3339 nanosecond, sourced at the producing service.
    pub timestamp: DateTime<Utc>,

    /// Monotonic sequence assigned by the journal writer.
    pub journal_seq: u64,

    /// Trace ID (ULID, 26-char Base32). Required per `principle-tracing-chains-end-to-end`.
    pub trace_id: String,

    /// Parent trace ID, when this event is part of a child trace.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_trace_id: Option<String>,

    /// Service name, Maestro session ID, or `human:<user-id>`.
    pub actor: String,

    /// Event-specific payload, validated against the generated JSON schema.
    pub payload: P,
}

impl<P> EventEnvelope<P> {
    /// Construct a new envelope with the current schema version and now-timestamp.
    ///
    /// Caller supplies `journal_seq` from the journal writer's sequence counter.
    pub fn new(
        event_type: impl Into<String>,
        event_subtype_version: u32,
        journal_seq: u64,
        trace_id: impl Into<String>,
        actor: impl Into<String>,
        payload: P,
    ) -> Self {
        Self {
            schema_version: ENVELOPE_SCHEMA_VERSION,
            event_type: event_type.into(),
            event_subtype_version,
            timestamp: Utc::now(),
            journal_seq,
            trace_id: trace_id.into(),
            parent_trace_id: None,
            actor: actor.into(),
            payload,
        }
    }

    /// Set the parent trace ID, marking this event as part of a child trace.
    #[must_use]
    pub fn with_parent_trace(mut self, parent_trace_id: impl Into<String>) -> Self {
        self.parent_trace_id = Some(parent_trace_id.into());
        self
    }
}

/// Generated per-event types and JSON schemas.
///
/// Populated by `tools/events-codegen.py` from `events.toml`. Empty in
/// Phase 0; codegen lands in `task-events-codegen-pipeline`.
pub mod generated {
    // Intentionally empty until codegen runs.
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn envelope_round_trips_through_json() {
        #[derive(Debug, Serialize, Deserialize, PartialEq)]
        struct TestPayload {
            field: String,
        }

        let envelope = EventEnvelope::new(
            "test.event",
            1,
            42,
            "01HXKJVF7P4N6X5R8SRZWB6JCM",
            "jam-svc-test",
            TestPayload {
                field: "hello".into(),
            },
        );

        let json = serde_json::to_string(&envelope).unwrap();
        let parsed: EventEnvelope<TestPayload> = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.event_type, "test.event");
        assert_eq!(parsed.event_subtype_version, 1);
        assert_eq!(parsed.journal_seq, 42);
        assert_eq!(parsed.trace_id, "01HXKJVF7P4N6X5R8SRZWB6JCM");
        assert_eq!(parsed.payload.field, "hello");
        assert!(parsed.parent_trace_id.is_none());
    }

    #[test]
    fn envelope_omits_parent_trace_when_root() {
        #[derive(Serialize, Deserialize)]
        struct Empty {}

        let envelope = EventEnvelope::new("test.event", 1, 1, "01ABC", "actor", Empty {});
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(!json.contains("parent_trace_id"));
    }

    #[test]
    fn envelope_includes_parent_trace_when_child() {
        #[derive(Serialize, Deserialize)]
        struct Empty {}

        let envelope = EventEnvelope::new("test.event", 1, 1, "01ABC", "actor", Empty {})
            .with_parent_trace("01PARENT");
        let json = serde_json::to_string(&envelope).unwrap();
        assert!(json.contains("\"parent_trace_id\":\"01PARENT\""));
    }
}
