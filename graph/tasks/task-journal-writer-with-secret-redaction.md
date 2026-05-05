---
id: task-journal-writer-with-secret-redaction
type: task
status: backlog
created: 2026-05-04T03:58:04.998794408Z
updated: 2026-05-04T04:09:06.039836827Z
edges:
- target: feat-substrate-services
  type: child_of
---
Phase 0 (§12). Journal writer subscribes to all `journal.*` subjects, writes rotated JSONL to `~/.jam/journal/YYYY-MM-DD/journal.<group>.jsonl`. Redacts known secret regex patterns at write-time (Anthropic `sk-ant-...`, OpenAI `sk-...`, GitHub PAT `ghp_...`, etc.).

Per `comp-orchestrator-jsonl-journal`.

Acceptance: a publish containing `sk-ant-...` lands in JSONL with `<redacted-secret>` substituted.