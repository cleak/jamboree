---
id: api-mcp-discover-and-load
type: api_surface
status: stable
created: 2026-05-04T03:53:00.937886126Z
updated: 2026-05-06T21:26:08Z
edges:
- target: comp-mcp-tool-router
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`mcp-discover-and-load(intent)` → loads MCP tools matching intent (§5.6, §4.9).

Tool Router pattern: instead of pre-registering every MCP tool with the Maestro (system-prompt bloat), this meta-tool lets the Maestro describe what it needs and load tools on demand.

The Python Maestro contract is `McpDiscoverAndLoadRequest(intent,
project="blueberry")` routed as `mcp-discover-and-load` on
`meta.mcp-discover-and-load`. The NATS trace header remains the source of
`trace_id`; the router logs selected servers and later MCP tool calls with the
same trace id.
