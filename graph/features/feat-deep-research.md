---
id: feat-deep-research
type: feature
status: draft
created: 2026-05-04T03:28:19.661944347Z
updated: 2026-05-04T04:13:54.748495932Z
owner: caleb
edges:
- target: comp-exa-backend
  type: uses
- target: comp-jam-svc-research
  type: uses
- target: comp-parallel-search-backend
  type: uses
- target: comp-perplexity-sonar-backend
  type: uses
- target: comp-research-completion-handler
  type: uses
- target: comp-tavily-backend
  type: uses
- target: jamboree-v5
  type: child_of
- target: principle-provider-agnostic-everywhere
  type: constrained_by
- target: principle-untrusted-content-cannot-issue-commands
  type: constrained_by
- target: task-jam-svc-research
  type: parent_of
---
Tiered access to provider research engines (§4.10). We don't build research infrastructure; we adapt to each provider's output.

Tiers:
- `Quick`: Tavily /research, ~$0.01-0.05, 5-30s.
- `Standard`: Perplexity Sonar Pro, ~$0.10-0.50, 30-120s.
- `Deep`: Exa Deep Research / Parallel Pro, ~$1-5, 5-15min.

Output convention (regardless of provider) at `~/.jam/research/<task-id>/`: `report.md`, `findings.json`, `sources.jsonl`, `transcript.jsonl`, `metadata.json`.

On completion, `research-completion-handler` reads `findings.json` and creates a Tempyr research node with stable ID, then emits `research.completed`. Other tasks can `query-tempyr` for it; the Maestro can cite it in Picker prompts.