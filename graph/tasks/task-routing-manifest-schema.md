---
id: task-routing-manifest-schema
type: task
status: backlog
created: 2026-05-04T04:00:12.870352660Z
updated: 2026-05-04T04:15:06.676153472Z
edges:
- target: feat-hot-patching
  type: child_of
---
Phase 7 (§12). Routing manifest schema in NATS KV.

Per `comp-routing-manifest`, `dec-tool-services-out-of-process`.

Acceptance: `jam patch apply <service> <version>` writes new manifest; Maestro re-reads on `routing-manifest.updated` events; next tool call uses new prefix.