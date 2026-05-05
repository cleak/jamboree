//! Trace propagation through NATS message headers.

use async_nats::header::{HeaderMap, HeaderName, HeaderValue};
use jam_trace::{TraceCtx, TraceId};
use std::str::FromStr;

/// Header name for the trace ID. Must be present on every NATS message
/// produced by Jamboree services (spec §23.3.1, §13.15).
pub const TRACE_ID_HEADER: &str = "Trace-Id";

/// Header name for the parent trace ID. Present only on child traces
/// (Picker spawn, patch apply, atomic-swap, research request).
pub const PARENT_TRACE_ID_HEADER: &str = "Parent-Trace-Id";

/// Stable origin_kind written into the [`TraceCtx`] returned by
/// [`extract_trace_from_headers`]. Receivers can use this to distinguish
/// "trace inherited from inbound message" from "trace opened locally".
pub const NATS_INBOUND_ORIGIN: &str = "nats.inbound";

/// Build NATS headers carrying the trace context.
///
/// `Trace-Id` is always written. `Parent-Trace-Id` is written iff the
/// context has a parent.
///
/// # Panics
///
/// Panics only if [`TraceId`]'s `Display` produces non-ASCII bytes — which
/// it can't (ULID is Crockford-Base32, always ASCII).
#[must_use]
pub fn build_trace_headers(ctx: &TraceCtx) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static(TRACE_ID_HEADER),
        HeaderValue::from_str(&ctx.trace_id.to_string()).expect("ulid is ascii"),
    );
    if let Some(parent) = ctx.parent_trace_id {
        headers.insert(
            HeaderName::from_static(PARENT_TRACE_ID_HEADER),
            HeaderValue::from_str(&parent.to_string()).expect("ulid is ascii"),
        );
    }
    headers
}

/// Extract a [`TraceCtx`] from inbound NATS headers.
///
/// Returns `None` if `Trace-Id` is missing or unparseable. Per
/// `principle-tracing-chains-end-to-end`, callers MUST refuse to process
/// messages that arrive without a valid `Trace-Id` (defense-in-depth on top
/// of the publish-side enforcement).
#[must_use]
pub fn extract_trace_from_headers(headers: &HeaderMap) -> Option<TraceCtx> {
    let trace_str = headers.get(TRACE_ID_HEADER)?.as_str();
    let trace_id = TraceId::from_str(trace_str).ok()?;

    let parent_trace_id = headers
        .get(PARENT_TRACE_ID_HEADER)
        .and_then(|v| TraceId::from_str(v.as_str()).ok());

    Some(TraceCtx {
        trace_id,
        parent_trace_id,
        origin_kind: NATS_INBOUND_ORIGIN,
        origin_summary: String::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_root_trace() {
        let ctx = TraceCtx::new_root("test.root", "summary");
        let headers = build_trace_headers(&ctx);
        let recovered = extract_trace_from_headers(&headers).expect("must extract");
        assert_eq!(recovered.trace_id, ctx.trace_id);
        assert!(recovered.parent_trace_id.is_none());
        assert_eq!(recovered.origin_kind, NATS_INBOUND_ORIGIN);
    }

    #[test]
    fn round_trips_child_trace() {
        let root = TraceCtx::new_root("test.root", "");
        let child = TraceCtx::child(&root, "test.child", "");
        let headers = build_trace_headers(&child);
        let recovered = extract_trace_from_headers(&headers).expect("must extract");
        assert_eq!(recovered.trace_id, child.trace_id);
        assert_eq!(recovered.parent_trace_id, Some(root.trace_id));
    }

    #[test]
    fn missing_trace_id_returns_none() {
        let headers = HeaderMap::new();
        assert!(extract_trace_from_headers(&headers).is_none());
    }

    #[test]
    fn malformed_trace_id_returns_none() {
        let mut headers = HeaderMap::new();
        headers.insert(
            HeaderName::from_static(TRACE_ID_HEADER),
            HeaderValue::from_str("not-a-valid-ulid").unwrap(),
        );
        assert!(extract_trace_from_headers(&headers).is_none());
    }

    #[test]
    fn parent_trace_invalid_falls_back_to_root() {
        let root = TraceCtx::new_root("test.root", "");
        let mut headers = build_trace_headers(&root);
        // Inject malformed parent.
        headers.insert(
            HeaderName::from_static(PARENT_TRACE_ID_HEADER),
            HeaderValue::from_str("garbage").unwrap(),
        );
        let recovered = extract_trace_from_headers(&headers).expect("trace_id still valid");
        assert_eq!(recovered.trace_id, root.trace_id);
        // Malformed parent silently drops to None — matches "best-effort
        // extraction" rather than rejecting the whole message.
        assert!(recovered.parent_trace_id.is_none());
    }
}
