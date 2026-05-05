---
id: dec-record-learning-emits-dual
type: decision
status: decided
created: 2026-05-04T03:46:38.657282533Z
updated: 2026-05-04T03:46:38.657283059Z
---
**`record-learning` writes both a markdown skill note AND a Tempyr decision/finding entry** (§5.5, §7.1, §22.7).

The double-write makes "why does this skill exist" trace-replayable from Tempyr's journal even after the skill itself is hot-edited or deleted.

Why over markdown-only: skills can be deleted or rewritten; Tempyr's append-only journal preserves the reasoning trail.