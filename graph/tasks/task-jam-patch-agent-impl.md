---
id: task-jam-patch-agent-impl
type: task
status: backlog
created: 2026-05-04T04:00:18.919444693Z
updated: 2026-05-04T04:15:24.419344482Z
edges:
- target: feat-hot-patching
  type: child_of
---
Phase 7 (§12). `jam-patch-agent` with pinned dependencies, focused LLM client.

Per `comp-patch-agent`, `dec-patch-agent-deterministic-then-llm`, `metric-patch-agent-llm-budget`.

Acceptance: apply a deliberately-broken patch; verify deterministic health checks catch it within 30s and trigger mechanical rollback. Apply a broken patch that mechanical rollback can't fix; verify LLM diagnosis runs, attempts, fails, ntfy human with incident dump.