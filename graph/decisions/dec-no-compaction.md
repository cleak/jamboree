---
id: dec-no-compaction
type: decision
status: decided
created: 2026-05-04T03:46:27.869230419Z
updated: 2026-05-04T05:03:12.501286805Z
edges:
- target: comp-orchestrator-jsonl-journal
  type: decision_for
- target: feat-substrate-services
  type: depended_on_by
---
**No journal compaction** (§4.4.3). The journal is sacred. Disk is cheap; replay-from-journal is the recovery story.

Old event types stay in journal forever; new code emits new types; both readable.

Captured as `principle-journal-is-sacred-no-compaction`.