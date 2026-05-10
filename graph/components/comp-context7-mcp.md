---
id: comp-context7-mcp
type: component
status: active
created: 2026-05-04T03:35:01.605595145Z
updated: 2026-05-06T21:20:00Z
edges:
- target: dec-context7-load-bearing-for-pickers
  type: has_decision
- target: feat-mcp-integration
  type: used_by
---
Context7 is **load-bearing for fast-moving-library workloads** (§4.9). Bevy ships breaking changes per minor release; LLM training data always lags. Context7 has version-pinned doc indexes (e.g. `bevy_ecs/0.16.1`, Tokio `1.49.0`) that solve the "model writes 0.13 patterns into a 0.16 codebase" failure mode.

Always-on for any Picker doing code work. Per memory, Context7 covers §4.9 MCP layer in the recommended initial deploy.

The Maestro itself does not need Context7 — delegate research-with-docs subtasks to a Picker rather than spending Maestro tool-call budget on doc lookups.

Implementation note (2026-05-06): the Blueberry project MCP config and
`jam-svc-session` launch adapters now pass enabled unauthenticated MCP servers
to Claude Code (`--mcp-config --strict-mcp-config`) and OpenCode. Unit coverage
verifies the generated configs include the remote Context7 endpoint when it is
enabled in `[mcp-servers]`.
