---
id: task-maestro-spawn-via-sudo
type: task
status: done
created: 2026-05-04T04:01:09.130076680Z
updated: 2026-05-06T09:08:42Z
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
    .current_dir("/")                               // accessible before sudo switches uid
    .env_clear()
    .envs(allowlist)
    ...;
```

Worktree path also changes: `~/.jam/worktrees/<task-id>/` → `/home/picker/workers/<task-id>/`. The worktree creation protocol (§6.9) still applies; only the root path changes.

Per `comp-jam-svc-session`, `feat-multi-user-security-model`.

Implementation note (2026-05-06): `crates/jam-svc-session` now builds Codex Picker launches through `sudo -n -u picker --preserve-env=<allowlist> -- <codex-bin> ...` when `JAM_SESSION_USE_SUDO` is enabled. The preserve list is derived from the same `PickerEnv` allowlist that is applied after `env_clear()`, so trace IDs, session/task IDs, budget, `HOME`, `CODEX_HOME`, `PATH`, and optional GitHub token propagation cannot drift from the actual launch environment. `crates/jam-svc-worktree` defaults the Picker worktree root to `/home/picker/workers`.

Operational note (2026-05-06): the sudo wrapper intentionally starts from `/`, not the Picker worktree. With the documented `picker:picker` 700/750 permissions, the pre-exec `current_dir` call would run as `maestro` and fail before `sudo` can switch users. The Codex adapter passes `--cd /home/picker/workers/<task-id>` so the harness enters the worktree after it is running as `picker`; `docs/security-setup.md` §7.2 now records this corrected pattern.

Verification (2026-05-06): unit tests cover the sudo argv shape, derived preserve-env allowlist, explicit Picker env values, and `/home/picker/workers` default. A live local smoke ran `sudo -n -u maestro ... sudo -n -u picker --preserve-env=...` with an env-cleared allowlist, verified the process was `picker`, then wrote inside `/home/picker/workers/jam-sudo-spawn-smoke`.
