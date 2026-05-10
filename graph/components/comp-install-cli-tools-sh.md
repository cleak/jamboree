---
id: comp-install-cli-tools-sh
type: component
status: active
created: 2026-05-04T03:40:00.103038324Z
updated: 2026-05-06T12:14:53.959780813Z
edges:
- target: comp-multi-user-filesystem-layout
  type: depends_on
- target: feat-multi-user-security-model
  type: used_by
---
`scripts/install-cli-tools.sh` — installs `@openai/codex` (npm), `@anthropic-ai/claude-code` (official native installer), and `opencode-ai` (npm) **per-user** for caleb/maestro/picker, plus daily auto-update cron (security-setup §4.5).

Per-user (not root) is required because harness tools update by writing to their install location — and Claude Code refuses to run at all — when that location is root-owned. Codex/Claude OAuth tokens land in each user's home (`~/.codex/auth.json`, `~/.claude/...`) with mode 700; OpenCode/DeepSeek gets its API key per spawn from Jamboree's secrets path.

Companion `cli-tools-update.sh` runs once a day per user via `/etc/cron.d/jam-cli-update` (staggered at 4:15 / 4:30 / 4:45 AM local). Uses `flock` to prevent concurrent runs, logs to `~/.cache/jam-cli-update.log`, isolates failures so a bad update only damages one user's installation.

WSL note: cron daemon doesn't start automatically. After install, run `sudo service cron start` and add to `/etc/wsl.conf` `[boot] command="service cron start"`.
