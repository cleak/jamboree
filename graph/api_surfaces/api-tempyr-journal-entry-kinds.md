---
id: api-tempyr-journal-entry-kinds
type: api_surface
status: draft
created: 2026-05-04T03:53:53.769177869Z
updated: 2026-05-04T04:59:41.114608355Z
edges:
- target: comp-tempyr-mcp-client-wrapper
  type: exposed_by
- target: feat-tempyr-knowledge-and-journal
  type: exposed_by
---
Tempyr's eight typed entry kinds (§22.1):
- `plan` (goals, acceptance_criteria)
- `finding`
- `assumption`
- `question`
- `decision` (chosen, rationale, reversible, detail ≥ 50 chars — required)
- `dead_end` (approach, failure_mode; convention `tags: [skill:<scope>]` for skill-suspicion)
- `risk`
- `outcome` (summary; `final: true` closes session)

Anchoring (§22.2): per-(worktree, agent) sessions. Picker anchors at own worktree (`agent: picker:<harness>:<handle>`); Maestro anchors at canonical worktree (`agent: maestro:<session-id>` per-wake unique).

Hybrid retrieval: BM25 + vec0 vector search + RRF + recency weighting + kind boost (§22.5).

Git-ref publishing: `tempyr journal flush` publishes session as `refs/tempyr/journals/archive/<YYYY>/<MM>/<DD>/<id>` (§22.1).

Lint: `tempyr journal lint` flags inconsistencies; `jam doctor` runs trace-gap detection corollary (§22.8).