---
id: feat-mcp-integration
type: feature
status: draft
created: 2026-05-04T03:28:19.298179817Z
updated: 2026-05-04T04:38:34.744305420Z
owner: caleb
edges:
- target: comp-composio-connect
  type: uses
- target: comp-context7-mcp
  type: uses
- target: comp-mcp-server-registry
  type: uses
- target: comp-mcp-tool-router
  type: uses
- target: dec-context7-load-bearing-for-pickers
  type: depends_on
- target: jamboree-v5
  type: child_of
- target: principle-adopt-subsystems-not-frameworks
  type: constrained_by
- target: principle-provider-agnostic-everywhere
  type: constrained_by
- target: principle-untrusted-content-cannot-issue-commands
  type: constrained_by
- target: task-composio-integration
  type: parent_of
- target: task-mcp-discover-and-load
  type: parent_of
- target: task-per-project-mcp-config
  type: parent_of
- target: task-untrusted-content-wrapping-mcp
  type: parent_of
---
Three pieces of plumbing become first-class (§4.9):

1. **Per-project MCP server registry** (`~/.jam/config/projects/<project>.toml`).
2. **Context7 always-on for any Picker doing code work** — Bevy ships breaking changes per minor release; LLM training data lags; Context7's version-pinned doc indexes (e.g. `bevy_ecs/0.16.1`, Tokio `1.49.0`) solve the "model writes 0.13 patterns into a 0.16 codebase" failure mode.
3. **Composio Connect for OAuth-managed services** (Linear, Slack, Notion, hundreds more) — single endpoint, many services.

**Tool Router pattern**: Instead of pre-registering every MCP tool with the Maestro (system-prompt bloat), expose meta-tool `mcp-discover-and-load(intent)` that lets the Maestro describe what it needs and load tools on demand.

All MCP responses pass through `Untrusted<String>` wrapping (§11.2.4) before the Maestro sees them.