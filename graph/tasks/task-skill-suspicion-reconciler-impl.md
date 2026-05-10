---
id: task-skill-suspicion-reconciler-impl
type: task
status: done
created: 2026-05-04T03:59:43.930114480Z
updated: 2026-05-06T09:54:36.572981062Z
edges:
- target: feat-self-improvement
  type: child_of
---
Phase 5 (§12). `skill-suspicion-reconciler` watching Tempyr `dead_end` accumulation hourly.

Per `comp-skill-suspicion-reconciler`, `metric-skill-suspicion-threshold`.

Acceptance: hand-craft a skill that's deliberately wrong; run a few Picker tasks that fail in ways logged as Tempyr `dead_end` entries with the skill tagged; verify reconciler emits `skill.under-suspicion` after threshold.

Implementation note (2026-05-06): crate `crates/jam-skill-suspicion` now
implements the deterministic reconciler. It runs `tempyr journal search --json
--kind dead_end --since-days <N> --limit <M> "*"`, counts `skill:<scope>` tags,
deduplicates matching entry ids, and publishes a traced
`evolve.skill-under-suspicion` event to `journal.evolve.skill-under-suspicion`
when the configured threshold is reached. Defaults match the decision
threshold: 3 dead ends in 7 days.

Live smoke (2026-05-06): temporary NATS on port `42513` plus
`jam-nats-bridge` and a fake `tempyr` returning three `dead_end` hits tagged
`skill:blueberry/fake-bad-skill` produced
`journal/2026-05-06/journal.evolve.jsonl` with
`event_type=evolve.skill-under-suspicion`, `dead_end_count=3`,
`since_days=7`, and the three implicating traces.
