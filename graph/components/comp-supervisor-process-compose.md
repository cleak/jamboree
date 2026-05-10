---
id: comp-supervisor-process-compose
type: component
status: active
created: 2026-05-04T03:31:46.291856720Z
updated: 2026-05-06T20:16:21Z
edges:
- target: comp-multi-user-filesystem-layout
  type: depends_on
- target: feat-failure-handling
  type: used_by
- target: feat-multi-user-security-model
  type: used_by
- target: feat-substrate-services
  type: used_by
---
`process-compose` manages process lifecycle (§4.4.8): NATS server, Maestro process, all tool services, all reconcilers, UI server, skill evolution pipeline, patch agent.

Health checks, restart policies, structured logging.

`process-compose.yaml` declares each service's user explicitly under the multi-user model (security-setup §7.4):

```yaml
processes:
  nats:
    command: /usr/local/bin/nats-server -c /home/maestro/.jam/config/nats.toml
    user: maestro
  maestro:
    command: /opt/jam/bin/jam-maestro
    user: maestro
    environment:
      - JAM_HOME=/home/maestro/.jam
  jam-svc-observe:
    command: /opt/jam/bin/jam-svc-observe
    user: maestro
  ...
```

`user:` directive requires process-compose to launch as root or via `sudo`; subprocesses run as the declared user.

Implementation status (2026-05-06): `process-compose.yaml` is present at the repo root with explicit `user: maestro` on all 24 declared processes and global `HOME`/`JAM_HOME` pointed at `/home/maestro`. The live service enablement set is still incremental; disabled future services retain the user directive so they are safe when enabled.

Operator note (2026-05-06): launch the supervisor itself with
`sudo /opt/jam/bin/process-compose up -f /home/caleb/jamboree/process-compose.yaml`.
Do not run process-compose as `maestro`; it must start as root so it can apply
the per-process `user: maestro` drops.
