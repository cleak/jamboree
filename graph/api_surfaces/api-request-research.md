---
id: api-request-research
type: api_surface
status: stable
created: 2026-05-04T03:52:58.581735154Z
updated: 2026-05-06T21:26:08Z
edges:
- target: comp-jam-svc-research
  type: exposed_by
- target: feat-maestro-tool-surface
  type: exposed_by
---
`request-research(question, tier, scope?, deadline?)` → `ResearchHandle` (§5.6, §4.10).

Tiers: Quick (Tavily, ~$0.01-0.05, 5-30s), Standard (Sonar Pro, ~$0.10-0.50, 30-120s), Deep (Exa deep-reasoning search / Parallel Pro, ~$1-5, 5-15min).

Result lands in `~/.jam/research/<task-id>/` as a uniform shape regardless of provider. `research-completion-handler` creates Tempyr research node on completion.

Implementation note (2026-05-06): the Maestro tool registry now exposes `request-research` as `tool.research.request-research` with generated Python request type `ResearchRequestResearchRequest`; the Rust service accepts the same closed contract in `crates/jam-tools-core/schemas/research/request-research.request.json`.

Handler note (2026-05-06): when `JAM_RESEARCH_TEMPYR_GRAPH_DIR` is set, completion also creates a stable `note-research-<research-id>` Tempyr node and publishes `journal.research.tempyr-node-created`.
