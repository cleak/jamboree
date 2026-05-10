---
id: task-hard-fs-network-isolation-tests
type: task
status: done
created: 2026-05-04T03:59:35.295437965Z
updated: 2026-05-06T17:00:52.911920295Z
edges:
- target: feat-sandboxing-profile-x-backend
  type: child_of
---
Phase 4 (§12) acceptance tests. Hard FS / network isolation.

Acceptance: Picker in `hardened × docker` cannot access files outside its worktree (verified by attempted `ls /` in the Picker turning up only the container's view). Performance regression vs `local × default` is acceptable for the task class (compile-heavy regression < 25%).

Acceptance evidence (2026-05-06):

- `scripts/smoke-docker-sandbox-backend.sh` proves the functional isolation path with live NATS: Docker Pickers run with `/work` as the only writable project mount, cannot see `/home/caleb`, and `hardened × docker` records `network_blocked=1` when attempting `wget http://example.org`.
- `scripts/bench-docker-sandbox-compile.sh` proves compile-heavy overhead against `/home/caleb/blueberry` using `blueberry-ops-base:latest` as the Picker-equivalent image. The checked run measured local cold `cargo check --bin blueberry` at 153.825s and Docker cold at 165.161s, a 7.4% regression against the 25% threshold.
