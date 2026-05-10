---
id: task-jam-svc-research
type: task
status: blocked
created: 2026-05-04T03:59:24.736142271Z
updated: 2026-05-06T19:21:00.244755020Z
edges:
- target: feat-deep-research
  type: child_of
---
Phase 3.5 (§12). `jam-svc-research` with Tavily/Sonar Pro/Exa-Deep tiers.

Per `comp-jam-svc-research`, `comp-research-completion-handler`.

Acceptance: `request-research(tier=deep)` creates `~/.jam/research/<task-id>/`, Tempyr node created on completion, journal recorded.

Implementation note (2026-05-06): the local service boundary is implemented. `crates/jam-svc-research` handles traced `tool.research.request-research`, validates the request contract, writes the §4.10 output file shape in explicit fake-provider mode, and publishes research requested/completed journal events. `scripts/smoke-research-service.sh` passed with live NATS plus `jam-nats-bridge`; the smoke verified the returned handle, all five output files, and a completed journal line.

Provider adapter note (2026-05-06): real HTTP adapters are now wired behind the same service boundary for Tavily `/research`, Perplexity `/v1/sonar` (`sonar`, `sonar-pro`, `sonar-reasoning-pro`), Exa `/search` with `type="deep-reasoning"`, and Parallel `/v1/tasks/runs` with processor `pro`. Focused mock tests cover create/poll/result normalization for Tavily and Parallel plus synchronous Sonar and Exa output normalization. Exa's older `/research/v1` path is avoided because current Exa docs mark it deprecated on 2026-05-01 and direct callers to `/search` with `type="deep-reasoning"`.

Completion-handler note (2026-05-06): `JAM_RESEARCH_TEMPYR_GRAPH_DIR` now enables deterministic Tempyr note creation after output files land. The handler reads `report.md`, `findings.json`, `sources.jsonl`, and `metadata.json`, writes `note-research-<research-id>` under `graph/notes/`, and publishes a traced `journal.research.tempyr-node-created` event. `scripts/smoke-research-service.sh` now verifies the fake deep request, all output files, the Tempyr note, and both completion journal entries.

Credential note (2026-05-06): `jam-svc-research` now reads provider credentials from env, `JAM_SECRETS_FILE`, or maestro pass. It accepts the canonical search secret keys from §11.3 (`jam/search/exa`, `jam/search/tavily`, `jam/search/perplexity`) plus research-specific aliases; `JAM_SECRETS_FILE` backend errors fail loudly per `principle-failure-surfaces-immediately`. Local runtime has `jam/search/exa` present in maestro pass, so deep-tier provider selection is no longer blocked on local credential discovery.

Blocked note (2026-05-06): full acceptance still needs a real provider execution and non-fake Tempyr node verification. I did not run a real Exa deep-reasoning request because it may consume provider quota. To finish acceptance, run `request-research(tier=deep)` against the real provider path during an approved quota window and verify `~/.jam/research/<id>/`, the Tempyr note, and the completion journal entries.
