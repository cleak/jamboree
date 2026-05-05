---
id: task-maestro-spawn-via-sudo
type: task
status: backlog
created: 2026-05-04T04:01:09.130076680Z
updated: 2026-05-04T04:01:09.130077320Z
---
Replace v5 spawn pattern with multi-user pattern (security-setup §7.2):

```rust
let cmd = Command::new("sudo")
    .args([
        "-n",                                       // non-interactive (NOPASSWD)
        "-u", "picker",
        "--preserve-env={preserve_env}",            // pass through specified env
        "--",
        harness_binary,
    ])
    .current_dir(&worktree_path)
    .env_clear()
    .envs(allowlist)
    ...;
```

Worktree path also changes: `~/.jam/worktrees/<task-id>/` → `/home/picker/workers/<task-id>/`. The worktree creation protocol (§6.9) still applies; only the root path changes.

Per `comp-jam-svc-session`, `feat-multi-user-security-model`.