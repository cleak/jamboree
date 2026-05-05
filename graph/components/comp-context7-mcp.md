---
id: comp-context7-mcp
type: component
status: planned
created: 2026-05-04T03:35:01.605595145Z
updated: 2026-05-04T05:02:07.507980314Z
edges:
- target: dec-context7-load-bearing-for-pickers
  type: has_decision
- target: feat-mcp-integration
  type: used_by
---
Context7 is **load-bearing for fast-moving-library workloads** (§4.9). Bevy ships breaking changes per minor release; LLM training data always lags. Context7 has version-pinned doc indexes (e.g. `bevy_ecs/0.16.1`, Tokio `1.49.0`) that solve the "model writes 0.13 patterns into a 0.16 codebase" failure mode.

Always-on for any Picker doing code work. Per memory, Context7 covers §4.9 MCP layer in the recommended initial deploy.

The Maestro itself does not need Context7 — delegate research-with-docs subtasks to a Picker rather than spending Maestro tool-call budget on doc lookups.