---
id: comp-jam-svc-research
type: component
status: active
created: 2026-05-04T03:35:04.713462575Z
updated: 2026-05-06T19:20:10Z
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
- `Deep`: Exa deep-reasoning search / Parallel Pro, ~$1-5, 5-15min, fallback Sonar Reasoning Pro.

`request-research(question, tier, scope?, deadline?)` returns a `ResearchHandle`.

Output convention regardless of provider — `~/.jam/research/<task-id>/`: `report.md`, `findings.json`, `sources.jsonl`, `transcript.jsonl`, `metadata.json`.

Implementation note (2026-05-06): `crates/jam-svc-research` now exists and serves traced `tool.research.request-research` requests. It validates `question`, `tier`, optional `scope`, and optional `deadline`, selects the configured tier provider, and writes the full output convention (`report.md`, `findings.json`, `sources.jsonl`, `transcript.jsonl`, `metadata.json`) while publishing `journal.research.requested` and `journal.research.completed`. `JAM_RESEARCH_FAKE_PROVIDER=true` still provides deterministic smoke coverage, and real HTTP adapters now cover Tavily `/research`, Perplexity `/v1/sonar`, Exa `/search` with `type="deep-reasoning"`, and Parallel `/v1/tasks/runs`. `scripts/smoke-research-service.sh` passed with live NATS plus `jam-nats-bridge`, proving request/reply, output files, and JSONL journal landing for the fake-provider path; real provider acceptance remains blocked on credentials and Tempyr completion handling.

Implementation note (2026-05-06): provider credential loading now matches the rest of the runtime secret model. Env vars still win, `JAM_SECRETS_FILE` is supported with fail-loud parse/read errors, and the service falls back to maestro pass keys such as `jam/search/exa`, `jam/search/tavily`, `jam/search/perplexity`, plus research-specific aliases for provider-only keys. Focused tests cover Exa selection from the file backend; local runtime currently has `jam/search/exa` present in maestro pass.
