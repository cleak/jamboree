---
id: comp-jam-svc-research
type: component
status: planned
created: 2026-05-04T03:35:04.713462575Z
updated: 2026-05-04T04:57:44.485446180Z
edges:
- target: api-request-research
  type: exposes
- target: comp-events-toml-and-codegen
  type: depends_on
- target: comp-jam-trace-crate
  type: depends_on
- target: comp-nats-jetstream
  type: depends_on
- target: comp-research-completion-handler
  type: depends_on
- target: comp-routing-manifest
  type: depends_on
- target: feat-deep-research
  type: used_by
- target: feat-maestro-tool-surface
  type: used_by
- target: feat-tool-services-out-of-process
  type: used_by
- target: principle-decoupled-processes-bus
  type: constrained_by
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
Tiered access to provider research engines (§4.10). Subject prefix `tool.research.*`. Crate `crates/jam-svc-research/`.

Tiers:
- `Quick`: Tavily `/research`, ~$0.01-0.05, 5-30s, fallback Sonar.
- `Standard`: Perplexity Sonar Pro, ~$0.10-0.50, 30-120s, fallback Sonar Reasoning Pro.
- `Deep`: Exa Deep Research / Parallel Pro, ~$1-5, 5-15min, fallback Sonar Reasoning Pro.

`request-research(question, tier, scope?, deadline?)` returns a `ResearchHandle`.

Output convention regardless of provider — `~/.jam/research/<task-id>/`: `report.md`, `findings.json`, `sources.jsonl`, `transcript.jsonl`, `metadata.json`.