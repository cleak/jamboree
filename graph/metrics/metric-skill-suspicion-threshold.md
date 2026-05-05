---
id: metric-skill-suspicion-threshold
type: metric
status: proposed
created: 2026-05-04T03:48:04.553838870Z
updated: 2026-05-04T03:48:04.553839488Z
---
**Skill-suspicion threshold**: ≥3 `dead_end` Tempyr entries within 7 days, tagged with the same skill (§7.4, §22.6).

Hourly `skill-suspicion-reconciler` query. Below threshold: noise. At threshold: emit `skill.under-suspicion`; Maestro reviews on next wake.

`dead_end` entry-kind already requires structured failure-mode + approach data, so the corpus is high-signal.