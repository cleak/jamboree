---
id: comp-seed-maestro-secrets-sh
type: component
status: active
created: 2026-05-04T03:40:01.606758134Z
updated: 2026-05-04T04:51:24.816572483Z
edges:
- target: comp-init-maestro-keyring-sh
  type: depends_on
- target: comp-pass-secret-backend
  type: depends_on
- target: feat-multi-user-security-model
  type: used_by
---
`scripts/seed-maestro-secrets.sh` — interactive, idempotent walk through the canonical secrets list (security-setup §5.3). Already in repo per CLAUDE.md mention.

Equivalent manual commands:
```bash
sudo -u maestro -i pass insert jam/pickers/github-app-id
sudo -u maestro -i pass insert jam/pickers/github-app-installation-id
sudo -u maestro -i pass insert -m jam/pickers/github-app-key
sudo -u maestro -i pass insert jam/notify/ntfy-token
sudo -u maestro -i pass insert jam/nats/token
sudo -u maestro -i pass insert jam/search/brave    # default starter only
```

Per memory: Brave is recommended primary for the §4.8 search-router; populate other backends only as workload demands.
