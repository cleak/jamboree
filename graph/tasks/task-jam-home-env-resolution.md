---
id: task-jam-home-env-resolution
type: task
status: backlog
created: 2026-05-04T04:01:12.249324525Z
updated: 2026-05-04T04:01:12.249325020Z
---
Define `JAM_HOME` env var that defaults to `/home/maestro/.jam` when running as maestro; resolves to `~/.jam` when running as caleb (for CLI). Tools and services use `JAM_HOME` everywhere instead of hardcoded `~/`.

Per security-setup §7.1 path defaults table.

Add a config:
```toml
# /home/maestro/.jam/config/skills.toml
skills-repo = "/home/caleb/code/jam-skills"
```

The Maestro reads this; the inotify watcher uses this path; CLI tools that read skills use this path.