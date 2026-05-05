//! Trace ID propagation for the Jamboree orchestrator.
//!
//! Per `principle-tracing-chains-end-to-end` and spec §23: every observable
//! behavior of the system traces backwards to its origin event without gaps.
//! This crate provides the [`TraceId`], [`TraceCtx`], and [`TracedPublish`]
//! primitives used throughout the substrate.
//!
//! ## One external trigger, one trace
//!
//! Per `principle-one-trigger-one-trace`: a trace opens when an external
//! trigger arrives (CLI command, wake event, periodic tick, webhook). Activity
//! within that trigger shares the trace via [`TraceCtx`]. When activity spawns
//! a child workflow with its own external visibility (Picker spawn, patch
//! apply, research request), [`TraceCtx::child`] opens a child trace whose
//! `parent_trace_id` points back at the original.

#![deny(missing_docs)]

use serde::{Deserialize, Serialize};
use ulid::Ulid;

/// A trace identifier. ULID format: 26-char Crockford-Base32, time-sortable,
/// globally unique.
///
/// Pattern: `^[0-9A-HJKMNP-TV-Z]{26}$`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct TraceId(Ulid);

impl TraceId {
    /// Generate a new ULID-backed trace ID.
    #[inline]
    pub fn new() -> Self {
        Self(Ulid::new())
    }

    /// Construct from an existing [`Ulid`] (rarely needed; prefer [`Self::new`]).
    #[inline]
    pub const fn from_ulid(ulid: Ulid) -> Self {
        Self(ulid)
    }

    /// Access the underlying ULID.
    #[inline]
    pub const fn as_ulid(&self) -> Ulid {
        self.0
    }
}

impl Default for TraceId {
    fn default() -> Self {
        Self::new()
    }
}

impl std::fmt::Display for TraceId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.fmt(f)
    }
}

impl std::str::FromStr for TraceId {
    type Err = TraceIdParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ulid::from_string(s)
            .map(Self)
            .map_err(|e| TraceIdParseError(e.to_string()))
    }
}

/// Failure parsing a [`TraceId`] from string form.
#[derive(Debug, thiserror::Error)]
#[error("invalid trace id: {0}")]
pub struct TraceIdParseError(String);

/// Trace context — propagated through every NATS message, tool call, and
/// journal entry.
///
/// `origin_kind` and `origin_summary` describe the external trigger that
/// opened the trace; they're frozen at trace creation and never overwritten.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceCtx {
    /// The trace ID for this activity.
    pub trace_id: TraceId,
    /// Pointer to the parent trace, when this is a child workflow.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_trace_id: Option<TraceId>,
    /// Stable kebab-case kind of the origin trigger
    /// (e.g. `"cli.task.spawn"`, `"pr.review-received"`).
    pub origin_kind: &'static str,
    /// Human-readable summary of the trigger, ≤ 200 chars typical.
    pub origin_summary: String,
}

impl TraceCtx {
    /// Open a new root trace from an external trigger.
    ///
    /// Used by CLI commands, wake handlers, periodic ticks, webhook receivers,
    /// reconciler scheduled runs, and any other source of external activity.
    pub fn new_root(origin_kind: &'static str, origin_summary: impl Into<String>) -> Self {
        Self {
            trace_id: TraceId::new(),
            parent_trace_id: None,
            origin_kind,
            origin_summary: origin_summary.into(),
        }
    }

    /// Open a child trace from a parent context.
    ///
    /// Used when a workflow spawns a sub-workflow with its own external
    /// visibility (Picker spawn, patch apply, research request, atomic-swap of
    /// a tool service).
    pub fn child(
        parent: &TraceCtx,
        origin_kind: &'static str,
        origin_summary: impl Into<String>,
    ) -> Self {
        Self {
            trace_id: TraceId::new(),
            parent_trace_id: Some(parent.trace_id),
            origin_kind,
            origin_summary: origin_summary.into(),
        }
    }

    /// True when this trace has no parent (i.e. opened by an external trigger).
    #[inline]
    pub const fn is_root(&self) -> bool {
        self.parent_trace_id.is_none()
    }
}

/// A publisher that requires a [`TraceCtx`] for every publish.
///
/// Per spec §23.3.1 and §23.6: raw `publish` is forbidden — clippy lint on
/// direct usage in non-trace crates. Bus subscribers extract trace from
/// headers (or top-level fields, depending on transport) and inject into the
/// request handler context.
///
/// Implementations live in transport-specific crates (e.g. `jam-nats` for the
/// NATS JetStream impl); this crate defines only the trait so that `jam-trace`
/// has no transport dependencies.
pub trait TracedPublish {
    /// Transport-specific error type.
    type Error;

    /// Publish `payload` to `subject`, attaching the trace context as headers
    /// (or equivalent envelope metadata).
    ///
    /// Implementations MUST refuse to publish without a [`TraceCtx`] —
    /// the type signature enforces this at compile time.
    fn publish_traced<T: Serialize>(
        &self,
        subject: &str,
        payload: &T,
        ctx: &TraceCtx,
    ) -> Result<(), Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    #[test]
    fn trace_id_is_26_chars() {
        let tid = TraceId::new();
        assert_eq!(tid.to_string().len(), 26);
    }

    #[test]
    fn trace_id_round_trips_through_string() {
        let tid = TraceId::new();
        let s = tid.to_string();
        let parsed = TraceId::from_str(&s).unwrap();
        assert_eq!(tid, parsed);
    }

    #[test]
    fn trace_id_rejects_garbage() {
        let result = TraceId::from_str("not-a-ulid");
        assert!(result.is_err());
    }

    #[test]
    fn root_ctx_has_no_parent() {
        let ctx = TraceCtx::new_root("test.root", "test root trace");
        assert!(ctx.is_root());
        assert!(ctx.parent_trace_id.is_none());
        assert_eq!(ctx.origin_kind, "test.root");
        assert_eq!(ctx.origin_summary, "test root trace");
    }

    #[test]
    fn child_ctx_links_to_parent_and_has_distinct_id() {
        let root = TraceCtx::new_root("test.root", "test root");
        let child = TraceCtx::child(&root, "test.child", "test child");
        assert!(!child.is_root());
        assert_eq!(child.parent_trace_id, Some(root.trace_id));
        assert_ne!(child.trace_id, root.trace_id);
        assert_eq!(child.origin_kind, "test.child");
    }

    #[test]
    fn trace_id_serde_round_trips() {
        let tid = TraceId::new();
        let json = serde_json::to_string(&tid).unwrap();
        let parsed: TraceId = serde_json::from_str(&json).unwrap();
        assert_eq!(tid, parsed);
    }

    #[test]
    fn trace_ctx_omits_parent_when_root() {
        let ctx = TraceCtx::new_root("test.root", "summary");
        let json = serde_json::to_string(&ctx).unwrap();
        assert!(!json.contains("parent_trace_id"));
    }

    #[test]
    fn trace_ctx_includes_parent_when_child() {
        let root = TraceCtx::new_root("test.root", "summary");
        let child = TraceCtx::child(&root, "test.child", "summary");
        let json = serde_json::to_string(&child).unwrap();
        assert!(json.contains("parent_trace_id"));
    }
}
