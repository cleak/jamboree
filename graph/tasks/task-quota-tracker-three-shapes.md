---
id: task-quota-tracker-three-shapes
type: task
status: backlog
created: 2026-05-04T03:59:13.425223891Z
updated: 2026-05-04T04:12:17.243865390Z
edges:
- target: feat-quota-tracking
  type: child_of
---
Phase 3 (§12). Quota tracker for all three harness shapes (Codex CLI 5h windows, Claude rate-limit, OpenCode/DeepSeek API budget).

Per `comp-quota-tracker`, `feat-quota-tracking`, `risk-quota-tracker-accuracy`.

Acceptance: burn Codex CLI quota by hand and watch the Maestro route subsequent tasks elsewhere.