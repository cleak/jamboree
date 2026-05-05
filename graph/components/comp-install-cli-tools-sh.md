---
id: comp-install-cli-tools-sh
type: component
status: active
created: 2026-05-04T03:40:00.103038324Z
updated: 2026-05-04T04:50:58.091725672Z
edges:
- target: comp-multi-user-filesystem-layout
  type: depends_on
- target: feat-multi-user-security-model
  type: used_by
---
`scripts/install-cli-tools.sh` — installs `@openai/codex` (npm) and `@anthropic-ai/claude-code` (official native installer) **per-user** for caleb/maestro/picker, plus daily auto-update cron (security-setup §4.5).

Per-user (not root) is required because both tools refuse to update — and Claude Code refuses to run at all — when their install location is root-owned. Each user's tokens land in their own home (`~/.codex/auth.json`, `~/.claude/...`) with mode 700, so the existing user-isolation boundary covers credential separation.

Companion `cli-tools-update.sh` runs once a day per user via `/etc/cron.d/jam-cli-update` (staggered at 4:15 / 4:30 / 4:45 AM local). Uses `flock` to prevent concurrent runs, logs to `~/.cache/jam-cli-update.log`, isolates failures so a bad update only damages one user's installation.

WSL note: cron daemon doesn't start automatically. After install, run `sudo service cron start` and add to `/etc/wsl.conf` `[boot] command="service cron start"`.