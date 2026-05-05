---
id: comp-sudoers-jam-users
type: component
status: active
created: 2026-05-04T03:40:04.058957763Z
updated: 2026-05-04T05:04:36.355579483Z
edges:
- target: comp-bootstrap-users-sh
  type: depended_on_by
- target: dec-convenience-first-multi-user
  type: has_decision
- target: feat-multi-user-security-model
  type: used_by
---
`/etc/sudoers.d/jam-users` mode 440 root:root (security-setup §3):

```
caleb    ALL=(maestro)    NOPASSWD: ALL
caleb    ALL=(maestro)    SETENV: ALL
caleb    ALL=(picker) NOPASSWD: ALL
caleb    ALL=(picker) SETENV: ALL

maestro  ALL=(picker)     NOPASSWD: ALL
maestro  ALL=(picker)     SETENV: ALL

Defaults!/usr/bin/* setenv
```

Validated with `visudo -c` before installing; bootstrap script refuses on validation failure.

What it allows: caleb→maestro and caleb→picker without password (ops + debug); maestro→picker for the orchestrator to spawn Pickers as the unprivileged user; SETENV on each transition (required to pass `JAM_TRACE_ID`/`JAM_PARENT_TRACE_ID`/secrets via `sudo --preserve-env=...`).

What it deliberately does not allow: picker→anything (least privilege); maestro→root or maestro→caleb (orchestrator never needs root); no command-level restrictions (relies on user-target restriction).

Cost of convenience: anyone who gets a shell as caleb (e.g., stolen SSH key) becomes maestro with no further auth. Acceptable for solo dev workstation.