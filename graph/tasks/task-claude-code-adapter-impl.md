---
id: task-claude-code-adapter-impl
type: task
status: done
created: 2026-05-04T03:59:05.067471465Z
updated: 2026-05-06T11:49:59.911971284Z
edges:
- target: feat-picker-layer-three-tier
  type: child_of
---
Phase 3 (§12). `ClaudeCodeAdapter` implementation of `HarnessAdapter`.

Per `comp-claude-code-adapter`, `api-harness-adapter-contract`.

Acceptance: spawn a Claude Code Picker via `--mcp` with project MCP servers; SessionStart/SessionEnd hooks fire `tempyr journal bootstrap/finalize`.

Implementation note (2026-05-06): `jam-svc-session` now treats `claude-code` as a live harness alongside `codex-cli`. The adapter verifies the `claude-code` lockfile pin, merges Tempyr SessionStart/SessionEnd hooks into the Picker worktree `.claude/settings.json`, converts enabled unauthenticated `[mcp-servers]` from `blueberry.toml` into a Claude `--mcp-config` JSON file, and launches `claude --print` with `--strict-mcp-config`. Unit coverage checks command construction, settings merge preservation, MCP conversion, auth-server refusal, and Claude version parsing. Live smoke used temporary NATS plus a fake `tool.worktree.create` responder: `tool.session.spawn-picker` returned a `claude-code:*` handle, real Claude Code 2.1.131 connected to the Tempyr MCP server, exited 0, and SessionEnd finalized a seeded Tempyr journal session by writing the `.ready` marker.
