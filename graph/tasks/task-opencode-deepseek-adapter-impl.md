---
id: task-opencode-deepseek-adapter-impl
type: task
status: backlog
created: 2026-05-04T03:59:07.840704027Z
updated: 2026-05-04T04:12:02.313352286Z
edges:
- target: feat-picker-layer-three-tier
  type: child_of
---
Phase 3 (§12). `OpenCodeAdapter` implementation of `HarnessAdapter`. Wraps OpenCode invocation with `tempyr journal bootstrap` prefix and `tempyr journal finalize` cleanup.

Per `comp-opencode-deepseek-adapter`.

Acceptance: spawn an OpenCode Picker with DeepSeek V4 Pro; verify Tempyr session opened+closed even on `full-stop` mid-task.