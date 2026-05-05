---
id: constraint-inotify-watches-524k
type: constraint
status: active
created: 2026-05-04T03:23:51.351857334Z
updated: 2026-05-04T04:28:54.226553060Z
edges:
- target: comp-jam-svc-knowledge
  type: constrains
- target: feat-live-update-flows
  type: constrains
---
`fs.inotify.max_user_watches` must be ≥ 524288 (`jam setup` check #5). The orchestrator watches ~50K files in normal operation: skills directory, Tempyr nodes/specs, per-Picker worktrees.

Fix per setup script:
```
echo 'fs.inotify.max_user_watches=524288' | sudo tee -a /etc/sysctl.d/99-jam.conf
sudo sysctl --system
```

Refusal at setup is per §2.12 — silent degradation (watcher misses events) is worse than upfront failure.