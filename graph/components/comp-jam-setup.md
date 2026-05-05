---
id: comp-jam-setup
type: component
status: planned
created: 2026-05-04T03:39:49.786056564Z
updated: 2026-05-04T05:03:22.149254129Z
edges:
- target: comp-multi-user-filesystem-layout
  type: depends_on
- target: comp-patch-agent
  type: depended_on_by
- target: dec-13-check-setup-script
  type: has_decision
- target: feat-jam-cli
  type: used_by
---
`jam setup` (and `jam doctor`) — preflight + ongoing health (§11.4). Refuses to install if environment isn't right. Every check has a specific error and a specific remediation hint per `principle-failure-surfaces-immediately`.

Full check list (13 from §11.4 + 11 multi-user additions from security-setup §10):

1. Linux kernel (refuse non-Linux outright; WSL detected via `/proc/version`)
2. JAM_HOME on native FS
3. Worktree-root on native FS
4. Tempyr canonical worktree on native FS
5. `fs.inotify.max_user_watches >= 524288`
6. systemd available
7. NTP synced (`timedatectl show -p NTPSynchronized` returns `yes`)
8. Clock skew vs NATS server < 1s
9. `pass` functional (test with synthetic `jam/setup-test-secret`)
10. `gpg-agent` running with working pinentry
11. NATS server reachable
12. Required harnesses installed at pinned versions (per `harnesses.lock`)
13. GitHub App key valid (test `octocrab` token exchange)
14. Service users `maestro` and `picker` exist with expected UIDs
15. Calling user is in `maestro` group (active in current shell)
16. `/etc/sudoers.d/jam-users` present and valid
17. `sudo -n -u maestro id` succeeds (NOPASSWD works)
18. `/etc/jam/bootstrap.log` present and matches expected version
19. JAM_HOME for current process is on native FS
20. Skills repo path exists and is readable by the running user
21. Canonical Tempyr worktree per active project has correct group ownership and setgid
22. maestro's pass store has the expected keys (per project config)
23. Picker spawn smoke test
24. picker cannot sudo (verify least privilege)

After setup succeeds, `setup-result.json` is written to NATS KV (`setup-result` bucket). Patch agent reads on first boot to know the verified-good baseline. Trace propagation health checks (`comp-trace-gap-detector`) are part of `jam doctor`.

Crate `crates/jam-setup/` provides `jam setup` and `jam doctor` binaries.