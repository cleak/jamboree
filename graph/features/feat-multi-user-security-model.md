---
id: feat-multi-user-security-model
type: feature
status: active
created: 2026-05-04T03:28:23.372008057Z
updated: 2026-05-04T05:55:14.262271287Z
owner: caleb
edges:
- target: comp-bootstrap-users-sh
  type: uses
- target: comp-init-maestro-keyring-sh
  type: uses
- target: comp-install-cli-tools-sh
  type: uses
- target: comp-multi-user-filesystem-layout
  type: uses
- target: comp-pass-secret-backend
  type: uses
- target: comp-seed-maestro-secrets-sh
  type: uses
- target: comp-sudoers-jam-users
  type: uses
- target: comp-supervisor-process-compose
  type: uses
- target: dec-adopt-blueberry-conventions
  type: depends_on
- target: dec-blueberry-jam-path
  type: depends_on
- target: dec-single-project-per-instance
  type: depends_on
- target: dec-skills-in-monorepo-v1
  type: depends_on
- target: jamboree-v5
  type: child_of
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-linux-only-deployment
  type: constrained_by
- target: principle-native-fs-only
  type: constrained_by
- target: task-prompt-injection-test
  type: parent_of
- target: the-manager
  type: serves
---
Three Linux user accounts and a sudoers config provide kernel-enforced filesystem isolation between human, orchestrator substrate, and Picker processes (security-setup.md).

Identities:
- `caleb` (UID 1000) — human; runs `jam` CLI; edits skills and Tempyr nodes.
- `maestro` (UID 2000) — substrate; runs NATS, Maestro, all `jam-svc-*` services, UI server.
- `picker` (UID 2001) — Pickers; sandboxed worktrees under `/home/picker/workers/<task-id>/` mode 700.

Sudoers `/etc/sudoers.d/jam-users`: NOPASSWD transitions caleb→maestro, caleb→picker, maestro→picker. SETENV on each.

Convenience-first posture: defends against prompt-injection-driven exfiltration and rogue Pickers. Does **not** defend against attacker who already has caleb's shell — acceptable for solo dev workstation.

Shared dirs (`~/code/blueberry-tempyr-live/`, `~/code/jam-skills/`) use mode 2770 with group `maestro` so the setgid bit propagates group ownership.

Setup: `bootstrap-users.sh` (idempotent, --dry-run/--verify-only) → manual GPG/pass init (security-setup §5) → `install-cli-tools.sh` per-user codex+claude-code + cron auto-update → `seed-maestro-secrets.sh`.