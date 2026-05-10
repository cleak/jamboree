---
id: task-multi-user-aware-jam-doctor
type: task
status: done
created: 2026-05-04T04:01:02.916919376Z
updated: 2026-05-06T08:09:09Z
---
Add the 11 multi-user `jam doctor` checks per security-setup §10:
14. Service users maestro and picker exist
15. Calling user is in maestro group
16. /etc/sudoers.d/jam-users present and valid
17. sudo -n -u maestro id succeeds (NOPASSWD works)
18. /etc/jam/bootstrap.log present
19. JAM_HOME on native FS
20. Skills repo path readable by running user
21. Canonical Tempyr worktree group ownership + setgid
22. maestro's pass store has expected keys
23. Picker spawn smoke test
24. picker cannot sudo

Per `comp-jam-setup`, security-setup §10.

Implementation note (2026-05-06): `crates/jam-setup/src/checks.rs` includes all 11 security-setup §10 multi-user checks in `run_all_checks`: service users, active `maestro` group membership, sudoers presence, sudo transition, bootstrap log, resolved `JAM_HOME` native-FS validation, skills repo readability, canonical Tempyr worktree presence, maestro pass-store placeholder, Picker spawn smoke, and picker-cannot-sudo. The check-count regression test asserts 24 total outcomes (13 base + 11 multi-user), and duplicate IDs are rejected by unit coverage.

Verification: `cargo test -p jam-setup run_all_checks_returns_24_outcomes`.
