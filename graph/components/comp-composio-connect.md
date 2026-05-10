---
id: comp-composio-connect
type: component
status: active
created: 2026-05-04T03:35:02.643575019Z
updated: 2026-05-06T21:10:00Z
edges:
- target: feat-mcp-integration
  type: used_by
---
Composio Connect for OAuth-managed services: Linear, Slack, Notion, Calendar, hundreds of others (§4.9). Single endpoint, many services. Composio handles OAuth, token refresh, scopes.

```toml
# ~/.jam/config/mcp-composio.toml
endpoint = "https://connect.composio.dev/mcp"
secret-key = "mcp/composio"
enabled-toolkits = ["linear", "slack", "notion"]
```

Implementation note (2026-05-06): `jam_maestro.project_config` now loads this
sidecar when present and expands each enabled toolkit into an MCP registry entry
such as `composio-linear` / `composio-slack`, carrying the shared endpoint,
`secret-key` auth ref, and toolkit metadata. The MCP router uses the toolkit
metadata to load Linear for `check linear ticket` without also loading unrelated
Composio toolkits.
