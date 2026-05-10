---
id: task-process-compose-yaml-multi-user
type: task
status: done
created: 2026-05-04T04:01:06.014901608Z
updated: 2026-05-06T20:16:21Z
---
Build `process-compose.yaml` with `user:` directive on each service so subprocesses run as the declared user (security-setup §7.4).

Per `comp-supervisor-process-compose`.

Process-compose launched as root or via `sudo`; subprocesses then run as maestro/picker as appropriate.

Implementation note (2026-05-06): `process-compose.yaml` now declares `user: maestro` on every long-running substrate process: NATS, the Maestro, UI server, tool services, reconcilers, and patch agent. Global environment includes `HOME=/home/maestro` and `JAM_HOME=/home/maestro/.jam`; Pickers remain spawned by `jam-svc-session` rather than process-compose. The file header now documents the root-launched `sudo /opt/jam/bin/process-compose up -f /home/caleb/jamboree/process-compose.yaml` path required for `user:` transitions.

Verification (2026-05-06): parsed the YAML with PyYAML, checked all 24 `processes` entries have a `user:` field, and ran `process-compose up -f <temp-all-disabled-config> --no-server -t=false --hide-disabled` with process-compose v1.40.1 to validate the config shape without starting real services.

Operator-doc note (2026-05-06): onboarding and security setup now show
root-launched `sudo /opt/jam/bin/process-compose up -f
/home/caleb/jamboree/process-compose.yaml`. Starting process-compose as
`maestro` is incorrect for this file because the `user:` directive requires the
supervisor process to have root privileges before dropping subprocesses to
`maestro`.
