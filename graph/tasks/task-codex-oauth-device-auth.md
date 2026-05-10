---
id: task-codex-oauth-device-auth
type: task
status: done
created: 2026-05-04T04:00:59.816130627Z
updated: 2026-05-06T09:26:30.796989747Z
---
Document and verify the `codex login --device-auth` flow as `maestro` user (security-setup §5.3).

Per `dec-chatgpt-subscription-oauth-for-maestro`. `--device-auth` is required because `maestro` (and `picker`) have no local browser session for default OAuth redirect — device-auth flow prints a one-time code to paste on a logged-in device instead.

Same Codex OAuth credential powers any Codex-based Picker, so no separate harness token needed for those.

Acceptance: `sudo -u maestro -i ls -la /home/maestro/.codex/auth.json` shows token file mode 600 owned by maestro:maestro.

Verification (2026-05-06): `docs/security-setup.md` §5.3 documents the `sudo -u maestro -i` then `codex login --device-auth` flow. Live check: `sudo -n -u maestro test -f /home/maestro/.codex/auth.json` succeeded, and `stat` reported `maestro:maestro 600 /home/maestro/.codex/auth.json`.
