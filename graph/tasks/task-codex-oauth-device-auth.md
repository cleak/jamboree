---
id: task-codex-oauth-device-auth
type: task
status: backlog
created: 2026-05-04T04:00:59.816130627Z
updated: 2026-05-04T04:00:59.816131132Z
---
Document and verify the `codex login --device-auth` flow as `maestro` user (security-setup §5.3).

Per `dec-chatgpt-subscription-oauth-for-maestro`. `--device-auth` is required because `maestro` (and `picker`) have no local browser session for default OAuth redirect — device-auth flow prints a one-time code to paste on a logged-in device instead.

Same Codex OAuth credential powers any Codex-based Picker, so no separate harness token needed for those.

Acceptance: `sudo -u maestro -i ls -la /home/maestro/.codex/auth.json` shows token file mode 600 owned by maestro:maestro.