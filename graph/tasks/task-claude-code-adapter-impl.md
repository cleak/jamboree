---
id: task-claude-code-adapter-impl
type: task
status: backlog
created: 2026-05-04T03:59:05.067471465Z
updated: 2026-05-04T04:11:54.623622440Z
edges:
- target: feat-picker-layer-three-tier
  type: child_of
---
Phase 3 (§12). `ClaudeCodeAdapter` implementation of `HarnessAdapter`.

Per `comp-claude-code-adapter`, `api-harness-adapter-contract`.

Acceptance: spawn a Claude Code Picker via `--mcp` with project MCP servers; SessionStart/SessionEnd hooks fire `tempyr journal bootstrap/finalize`.