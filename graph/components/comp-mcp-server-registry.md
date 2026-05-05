---
id: comp-mcp-server-registry
type: component
status: planned
created: 2026-05-04T03:35:00.444798719Z
updated: 2026-05-04T04:12:42.628636536Z
edges:
- target: feat-mcp-integration
  type: used_by
---
Per-project MCP server registry, config-driven (§4.9). Different projects might need different MCPs. Lives in `~/.jam/config/projects/<project>.toml`:

```toml
[mcp-servers]
context7 =     { url = "https://mcp.context7.com/mcp/v1", enabled = true }
github-mcp =   { url = "https://api.githubcopilot.com/mcp/", enabled = true, auth = "github-pat" }
warpgrep =     { url = "stdio:warpgrep", enabled = false }
tavily-mcp =   { url = "https://mcp.tavily.com/v1", enabled = false }
tempyr =       { url = "stdio:tempyr --mcp", enabled = true }  # always enabled
```

Both Maestro and Pickers see the same registry. Pickers that support MCP (Codex CLI, OpenCode, Claude Code with `--mcp`) get the relevant servers passed via their respective config mechanisms.