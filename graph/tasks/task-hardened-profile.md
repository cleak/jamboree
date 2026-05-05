---
id: task-hardened-profile
type: task
status: backlog
created: 2026-05-04T03:59:29.578454877Z
updated: 2026-05-04T04:13:04.245971366Z
edges:
- target: feat-sandboxing-profile-x-backend
  type: child_of
---
Phase 4 (§12). Hardened profile: minimal HOME, restricted env, outbound allowlist.

Per `feat-sandboxing-profile-x-backend`.

Acceptance: Picker in `hardened × docker` cannot reach disallowed domains (verified by attempting `curl https://example.org` failing).