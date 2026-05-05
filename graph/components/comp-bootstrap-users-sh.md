---
id: comp-bootstrap-users-sh
type: component
status: active
created: 2026-05-04T03:39:58.766624457Z
updated: 2026-05-04T04:50:50.009855229Z
edges:
- target: comp-multi-user-filesystem-layout
  type: depends_on
- target: comp-sudoers-jam-users
  type: depends_on
- target: feat-multi-user-security-model
  type: used_by
- target: principle-failure-surfaces-immediately
  type: constrained_by
---
`scripts/bootstrap-users.sh` — idempotent one-time bash script that establishes the multi-user layout (security-setup §4). Already exists in repo per CLAUDE.md.

Usage:
```bash
sudo ./bootstrap-users.sh                  # interactive, uses $SUDO_USER
sudo ./bootstrap-users.sh --user caleb     # explicit
sudo ./bootstrap-users.sh --dry-run        # preview only
sudo ./bootstrap-users.sh --verify-only    # audit existing setup, no changes
```

What it does (§4.2):
1. Preflight checks (root, Linux, valid human user with home on native FS).
2. Creates service users (maestro UID 2000, picker UID 2001) with `useradd`. Skips if already correct; warns on UID mismatch.
3. Adds human user to maestro group.
4. Normalizes `/home/caleb` to mode 751.
5. Writes sudoers config; validates with `visudo -c` before installing.
6. Prepares maestro/picker home directory scaffolding.
7. Writes `/etc/jam/bootstrap.log` audit record.
8. Verification phase.

What it does not do (§4.3): GPG/pass init (manual §5), state migration (§8), service installation (`jam setup`'s job), CLI tool installation (`install-cli-tools.sh`).

Mirrors the bootstrap script's pass/fail/info/warn/die helpers and "Fix:" remediation block style — same pattern `jam doctor` uses.