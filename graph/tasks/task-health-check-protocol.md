---
id: task-health-check-protocol
type: task
status: backlog
created: 2026-05-04T04:00:24.926480433Z
updated: 2026-05-04T04:15:41.233749974Z
edges:
- target: feat-hot-patching
  type: child_of
---
Phase 7 (§12). Health check protocol per service. Each tool service health-pings on `tool.<service>.ping` every 5s.

Per `feat-tool-services-out-of-process`.