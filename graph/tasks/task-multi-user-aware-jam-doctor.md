---
id: task-multi-user-aware-jam-doctor
type: task
status: backlog
created: 2026-05-04T04:01:02.916919376Z
updated: 2026-05-04T04:01:02.916919912Z
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