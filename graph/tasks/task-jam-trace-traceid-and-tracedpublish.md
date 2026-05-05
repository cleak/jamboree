---
id: task-jam-trace-traceid-and-tracedpublish
type: task
status: backlog
created: 2026-05-04T03:58:00.597181641Z
updated: 2026-05-04T04:08:52.204594247Z
edges:
- target: feat-substrate-services
  type: child_of
---
Phase 0 (§12). Implement `crates/jam-trace/` with `TraceId` (ULID), `TraceCtx`, propagation helpers, NATS publish wrapper that requires `trace_id` header.

Per `comp-jam-trace-crate`, `comp-traced-publish-wrapper`, `principle-tracing-chains-end-to-end`.

Acceptance: raw `nats.publish` is forbidden via clippy lint; `publish_traced` requires a `&TraceCtx` parameter; bus subscribers extract trace from headers.