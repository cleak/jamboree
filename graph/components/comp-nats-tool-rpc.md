---
id: comp-nats-tool-rpc
type: component
status: planned
created: 2026-05-04T03:31:34.604961662Z
updated: 2026-05-04T04:06:10.743738202Z
edges:
- target: feat-tool-services-out-of-process
  type: used_by
---
NATS request-reply contract for every tool (§4.3):
- Request subject: `tool.<service>.<method>`
- Request headers: `Trace-Id` (required), `Parent-Trace-Id` (optional), `Schema-Version` (required), `Reply-To` (auto-set by NATS).
- Request payload: JSON object matching the input schema for that tool.
- Reply: JSON object matching the output schema, OR a typed error `{"error": {"kind": ..., "detail": ..., "trace_id": ...}}`.

If routing manifest changes mid-call (atomic-swap during execution), the in-flight call completes against the old version; the next call uses the new version (§20.3).

Tools exposed by each service are JSON-schema-described in `crates/jam-tools-core/schemas/<service>/<tool>.json`.