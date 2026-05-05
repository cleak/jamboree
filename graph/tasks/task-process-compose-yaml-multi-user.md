---
id: task-process-compose-yaml-multi-user
type: task
status: backlog
created: 2026-05-04T04:01:06.014901608Z
updated: 2026-05-04T04:01:06.014902114Z
---
Build `process-compose.yaml` with `user:` directive on each service so subprocesses run as the declared user (security-setup §7.4).

Per `comp-supervisor-process-compose`.

Process-compose launched as root or via `sudo`; subprocesses then run as maestro/picker as appropriate.