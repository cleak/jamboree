---
id: api-reply-to-comment
type: api_surface
status: draft
created: 2026-05-04T03:52:25.067329866Z
updated: 2026-05-04T04:55:29.917506644Z
edges:
- target: comp-jam-svc-repo
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`reply-to-comment(artifact-id, text)` → posts reply via reviewer adapter (§5.4). Capability-gated: only when the reviewer adapter's `supports_reply == true`.