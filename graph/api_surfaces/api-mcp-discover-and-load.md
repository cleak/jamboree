---
id: api-mcp-discover-and-load
type: api_surface
status: draft
created: 2026-05-04T03:53:00.937886126Z
updated: 2026-05-04T04:57:52.920545898Z
edges:
- target: comp-mcp-tool-router
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`mcp-discover-and-load(intent)` → loads MCP tools matching intent (§5.6, §4.9).

Tool Router pattern: instead of pre-registering every MCP tool with the Maestro (system-prompt bloat), this meta-tool lets the Maestro describe what it needs and load tools on demand.