---
id: task-composio-integration
type: task
status: blocked
created: 2026-05-04T04:00:36.190608074Z
updated: 2026-05-06T21:10:00Z
edges:
- target: feat-mcp-integration
  type: child_of
---
Phase 8 (§12). Composio integration.

Per `comp-composio-connect`.

Implementation note (2026-05-06): the local config boundary is implemented.
`~/.jam/config/mcp-composio.toml` with `endpoint`, `secret-key`, and
`enabled-toolkits` is loaded by the Maestro project config layer; enabled
toolkits are exposed as typed MCP registry entries like `composio-linear` and
`composio-slack`. Router scoring now respects those toolkit tags, so a Linear
intent does not load unrelated Composio toolkits. Unit coverage verifies
sidecar loading, conflict failure, and Linear-only discovery.

Blocked note (2026-05-06): no real Composio credential/config is present.
Environment has no Composio key, and the runtime pass store needs the canonical
`jam/mcp/composio` secret for the sidecar's `secret-key = "mcp/composio"` auth
ref. To finish acceptance, create the Composio Connect endpoint/config, seed
the Composio secret, then call a real toolkit through the MCP client path.
