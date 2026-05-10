---
id: oq-systemd-unit
type: open_question
status: open
created: 2026-05-04T03:47:48.301828504Z
updated: 2026-05-06T20:16:21Z
---
**Future systemd unit `jam.service` for the root-launched supervisor** (security-setup §6.1).

Out of scope for the security-setup addendum. Current `process-compose.yaml`
uses per-process `user: maestro`, so the supervisor itself must launch as root
and then drop each service to `maestro`.

To-be-implemented in a later phase. Current pattern:
`sudo /opt/jam/bin/process-compose up -f /home/caleb/jamboree/process-compose.yaml`
with `process-compose down/list` for stop/status. A future unit should preserve
that root-launch + per-service user-drop model rather than running
process-compose itself as `maestro`.
