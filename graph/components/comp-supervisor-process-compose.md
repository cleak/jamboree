---
id: comp-supervisor-process-compose
type: component
status: planned
created: 2026-05-04T03:31:46.291856720Z
updated: 2026-05-04T04:51:32.916540444Z
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