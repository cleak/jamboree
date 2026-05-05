---
id: metric-fetch-staleness-threshold
type: metric
status: proposed
created: 2026-05-04T03:48:08.543461901Z
updated: 2026-05-04T03:48:08.543462426Z
---
**Worktree-create fetch-staleness threshold**: 60s (§6.9). When spawning multiple Pickers in quick succession, only the first triggers a `git fetch`; rest skip.

Configurable via `~/.jam/config/projects/<project>.toml [trunk] fetch-staleness-secs`.