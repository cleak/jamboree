---
id: task-mcp-discover-and-load
type: task
status: backlog
created: 2026-05-04T04:00:39.218157209Z
updated: 2026-05-04T04:16:27.204243649Z
edges:
- target: feat-mcp-integration
  type: child_of
---
Phase 8 (§12). `mcp-discover-and-load` meta-tool.

Per `comp-mcp-tool-router`, `api-mcp-discover-and-load`.

Acceptance: Maestro calls `mcp-discover-and-load(intent="check linear ticket")`; correct toolkit loads; Maestro calls the toolkit; journal records the call with trace_id.