---
id: task-untrusted-content-wrapping-mcp
type: task
status: backlog
created: 2026-05-04T04:00:42.271208755Z
updated: 2026-05-04T04:16:35.969409275Z
edges:
- target: feat-mcp-integration
  type: child_of
---
Phase 8 (§12). Untrusted-content wrapping for all MCP responses.

Acceptance: MCP server returning a prompt-injection payload — verify it's wrapped in `Untrusted` and Maestro doesn't act on it.