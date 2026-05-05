---
id: comp-multi-user-filesystem-layout
type: component
status: active
created: 2026-05-04T03:40:05.422988732Z
updated: 2026-05-04T05:04:55.388126056Z
edges:
- target: comp-bootstrap-users-sh
  type: depended_on_by
- target: comp-init-maestro-keyring-sh
  type: depended_on_by
- target: comp-install-cli-tools-sh
  type: depended_on_by
- target: comp-jam-setup
  type: depended_on_by
- target: comp-source-vs-runtime-bridge
  type: depended_on_by
- target: comp-supervisor-process-compose
  type: depended_on_by
- target: dec-no-docker-required
  type: has_decision
- target: feat-multi-user-security-model
  type: used_by
- target: principle-native-fs-only
  type: constrained_by
---
Filesystem layout per security-setup §2:

```
/home/caleb/                                    mode 751   caleb:caleb
├── .ssh/, .gnupg/, .password-store/, .config/  mode 700   caleb:caleb
├── .jam/                                       mode 750   caleb:caleb
└── code/
    ├── blueberry/                              mode 755   caleb:caleb        (PRISTINE — maestro never writes here)
    ├── blueberry-tempyr-live/                  mode 2770  caleb:maestro    (canonical worktree, shared)
    └── jam-skills/                             mode 2770  caleb:maestro    (skills git repo, shared)

/home/maestro/                                  mode 750   maestro:maestro
├── .gnupg/, .password-store/                   mode 700   maestro:maestro
└── .jam/                                       mode 750   maestro:maestro
    ├── config/, journal/, session-store.db, research/, incidents/,
    ├── maestro-aborted-sessions/, skills-evolution-candidates/, staging/,
    ├── nats-data/, tempyr-update-queue.jsonl, harness-update-queue.jsonl

/home/picker/                                   mode 750   picker:picker
└── workers/
    └── <task-id>/                              mode 700   picker:picker  (per-Picker isolation)

/etc/sudoers.d/jam-users                        mode 440   root:root
/etc/jam/bootstrap.log                          mode 644   root:root      (audit)
```

Key permission decisions:
- `/home/caleb` mode 751 lets maestro traverse to known shared subdirs without enumerating the rest.
- Shared dirs use mode 2770 with group maestro. setgid bit means new files inherit group.
- Per-Picker worktrees mode 700 prevent one Picker from reading another's mid-task state even though all share UID picker.