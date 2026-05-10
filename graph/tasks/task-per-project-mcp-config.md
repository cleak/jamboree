---
id: task-per-project-mcp-config
type: task
status: done
created: 2026-05-04T04:00:33.170409131Z
updated: 2026-05-06T09:32:16Z
edges:
- target: feat-mcp-integration
  type: child_of
---
Phase 8 (§12). Per-project MCP config in `~/.jam/config/projects/<name>.toml`.

Per `comp-mcp-server-registry`.

Implemented the Blueberry v1 project config loader in `jam_maestro.project_config`.
It reads `~/.jam/config/projects/blueberry.toml` by default, accepts a strict
`[mcp-servers]` table, validates server URL/auth fields, preserves disabled
servers, and exposes `enabled_mcp_servers()` for runtime handoff to MCP-aware
Picker launch paths.
