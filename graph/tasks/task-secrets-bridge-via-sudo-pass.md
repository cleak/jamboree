---
id: task-secrets-bridge-via-sudo-pass
type: task
status: backlog
created: 2026-05-04T04:01:15.382725992Z
updated: 2026-05-04T04:01:15.382726498Z
---
For caleb-side CLI to read NATS tokens etc. — implement `sudo -n -u maestro -i pass show <key>` pattern (security-setup §6.2, §7.3 Option B).

Per `oq-cli-sudo-vs-duplicate-nats-token` — open question whether to do this on every CLI command or duplicate token in caleb's pass.