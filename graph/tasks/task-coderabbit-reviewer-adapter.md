---
id: task-coderabbit-reviewer-adapter
type: task
status: backlog
created: 2026-05-04T03:58:52.133689003Z
updated: 2026-05-04T04:11:17.423834884Z
edges:
- target: feat-reviewer-adapters
  type: child_of
---
Phase 2 (§12). CodeRabbit reviewer adapter implementing `ReviewerAdapter` trait.

Per `comp-coderabbit-adapter`, `comp-reviewer-adapter-trait`.

Acceptance: PR with CodeRabbit comments: Maestro reads them, classifies them, decides which to address, dispatches a Picker with the reasoning, marks them handled.