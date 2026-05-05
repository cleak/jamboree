---
id: feat-live-update-flows
type: feature
status: draft
created: 2026-05-04T03:28:23.367583664Z
updated: 2026-05-04T04:22:33.879904085Z
owner: caleb
edges:
- target: comp-jam-svc-knowledge
  type: uses
- target: comp-nats-jetstream
  type: uses
- target: comp-pr-status-poller
  type: uses
- target: comp-trunk-fetcher
  type: uses
- target: comp-world-snapshot-cache
  type: uses
- target: constraint-inotify-watches-524k
  type: constrained_by
- target: constraint-ntp-sync-required
  type: constrained_by
- target: jamboree-v5
  type: child_of
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-observable-not-deterministic
  type: constrained_by
---
Catalog of bus subjects, event-driven invalidation, polling cadences (§21).

Cache invalidation (§21.2): observation service subscribes to events implying staleness (`pr.review-received{task_id}`, `pr.ci.status-changed{task_id}`, `pr.merged{task_id}`, `picker.exited{task_id}`, `picker.spawned{task_id}`, `branch.trunk-moved`, `tempyr.node-changed`, `harness.version-changed`, `quota.<harness>.<event>`). 60s TTL backstop.

Polling cadences (§21.3) where event subscription isn't available:
- `trunk-fetcher` 5min, `pr-status-poller` 30s/PR (adaptive: 5min for inactive PRs), `clock-watcher` 10min, `harness-version-watcher` 1h, `skill-suspicion-reconciler` 1h, evolution pipeline 1 week, Maestro periodic tick 5min.

File watchers (§21.4): inotify on `~/.jam/skills/` and Tempyr's source dirs (`~/code/<project>-tempyr-live/tempyr/nodes/` + `tempyr/specs/`). `fs.inotify.max_user_watches >= 524288`.

Skill update flow (§21.5): edit → inotify → emit `skills.changed{file_path}` → Maestro skill cache marked dirty → re-read on next `read-skills(scope)`. No restart.