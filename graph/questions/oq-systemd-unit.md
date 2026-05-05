---
id: oq-systemd-unit
type: open_question
status: open
created: 2026-05-04T03:47:48.301828504Z
updated: 2026-05-04T03:47:48.301829040Z
---
**Future systemd unit `jam.service` running under maestro** (security-setup §6.1).

Out of scope for the security-setup addendum. A wrapper that calls `sudo -u maestro /opt/jam/bin/jam start` would work as a stopgap; a real unit (running under maestro via systemd's `User=` directive) is the eventual answer.

To-be-implemented in a later phase. Current pattern: `sudo -u maestro -i jam start` / `jam stop` / `jam status`.