---
id: dec-skill-suspicion-threshold-3-in-7d
type: decision
status: decided
created: 2026-05-04T03:46:40.241165403Z
updated: 2026-05-04T05:03:49.898498732Z
edges:
- target: comp-skill-suspicion-reconciler
  type: decision_for
---
**Skill-suspicion threshold: 3+ `dead_end` entries within 7 days** (§7.4, §22.6).

Why this threshold: failures naturally cluster around bad skills. Three is enough to filter noise; seven days is long enough to capture pattern, short enough to be timely.

Maestro sees `skill.under-suspicion` event on next wake; decides to flag for evolution / deprecate / ignore. We don't auto-quarantine. The `dead_end` entry-kind already requires structured `failure-mode` + `approach` data, so the corpus is high-signal.

Tags-as-skill-references is a convention (`skill:<scope>`) that doesn't require Tempyr changes.