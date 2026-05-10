---
id: comp-jam-trace-crate
type: component
status: active
created: 2026-05-04T03:39:44.447233997Z
updated: 2026-05-06T21:15:00Z
edges:
- target: comp-jam-cli-binary
  type: depended_on_by
- target: comp-jam-svc-evolve
  type: depended_on_by
- target: comp-jam-svc-knowledge
  type: depended_on_by
- target: comp-jam-svc-message
  type: depended_on_by
- target: comp-jam-svc-observe
  type: depended_on_by
- target: comp-jam-svc-repo
  type: depended_on_by
- target: comp-jam-svc-research
  type: depended_on_by
- target: comp-jam-svc-search
  type: depended_on_by
- target: comp-jam-svc-session
  type: depended_on_by
- target: comp-jam-svc-supervise
  type: depended_on_by
- target: comp-jam-svc-worktree
  type: depended_on_by
- target: dec-trace-id-load-bearing
  type: has_decision
- target: dec-ulid-for-trace-ids
  type: has_decision
- target: feat-maestro-tool-surface
  type: used_by
- target: feat-trace-propagation
  type: used_by
- target: principle-one-trigger-one-trace
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
TraceId + propagation helpers (§23.2). Crate `crates/jam-trace/`.

```rust
pub struct TraceId(Ulid);  // 26-char Base32 ULID, time-sortable
impl TraceId {
    pub fn new() -> Self { Self(Ulid::new()) }
    pub fn from_str(s: &str) -> Result<Self> { ... }
}

pub struct TraceCtx {
    pub trace_id: TraceId,
    pub parent_trace_id: Option<TraceId>,
    pub origin_kind: &'static str,
    pub origin_summary: String,
}
```

Provides:
- `TraceCtx::new_root(kind, summary)` — open a new root trace.
- `TraceCtx::child(parent)` — open a child trace.
- `TraceCtx::from_nats_headers(headers)` — extract from NATS message.
- `TracedPublish` trait (§23.3.1) — NATS publish wrapper that requires trace_id.

Static enforcement (§23.6): event-emit helpers require `TraceCtx` parameter (no `Option<TraceId>`); raw `publish` is forbidden via clippy lint on direct usage in non-trace crates.
