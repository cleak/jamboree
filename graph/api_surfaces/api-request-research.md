---
id: api-request-research
type: api_surface
status: draft
created: 2026-05-04T03:52:58.581735154Z
updated: 2026-05-04T04:57:44.485446634Z
edges:
- target: comp-jam-svc-research
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`request-research(question, tier, scope?, deadline?)` → `ResearchHandle` (§5.6, §4.10).

Tiers: Quick (Tavily, ~$0.01-0.05, 5-30s), Standard (Sonar Pro, ~$0.10-0.50, 30-120s), Deep (Exa Deep / Parallel Pro, ~$1-5, 5-15min).

Result lands in `~/.jam/research/<task-id>/` as a uniform shape regardless of provider. `research-completion-handler` creates Tempyr research node on completion.