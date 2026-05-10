---
id: task-mcp-discover-and-load
type: task
status: done
created: 2026-05-04T04:00:39.218157209Z
updated: 2026-05-06T09:38:26Z
edges:
- target: feat-mcp-integration
  type: child_of
---
Phase 8 (§12). `mcp-discover-and-load` meta-tool.

Per `comp-mcp-tool-router`, `api-mcp-discover-and-load`.

Acceptance: Maestro calls `mcp-discover-and-load(intent="check linear ticket")`; correct toolkit loads; Maestro calls the toolkit; journal records the call with trace_id.

Implemented in `jam_maestro.mcp_router` and registered in
`MaestroToolRegistry` as `mcp-discover-and-load` on
`meta.mcp-discover-and-load`. The router scores enabled per-project MCP
servers by intent, loads matching server configs, and logs the discovery and
tool-call boundary to Tempyr journal entries tagged with `trace:<trace_id>`.
Unit coverage exercises the `check linear ticket` path against a Composio MCP
server and verifies traced journaling.
