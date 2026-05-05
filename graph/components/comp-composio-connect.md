---
id: comp-composio-connect
type: component
status: planned
created: 2026-05-04T03:35:02.643575019Z
updated: 2026-05-04T04:12:58.627702587Z
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