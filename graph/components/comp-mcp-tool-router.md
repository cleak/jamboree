---
id: comp-mcp-tool-router
type: component
status: planned
created: 2026-05-04T03:35:03.689564127Z
updated: 2026-05-04T04:57:52.920545423Z
edges:
- target: api-mcp-discover-and-load
  type: exposes
- target: feat-mcp-integration
  type: used_by
---
**Dynamic MCP tool loading via Tool Router pattern** (§4.9). Instead of pre-registering every MCP tool with the Maestro (which inflates the system prompt), expose a meta-tool `mcp-discover-and-load(intent)` that lets the Maestro describe what it needs and load tools on demand.

Untrusted-content handling: all MCP responses pass through `Untrusted<String>` wrapping (§11.2.4) before the Maestro sees them.