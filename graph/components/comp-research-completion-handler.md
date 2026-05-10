---
id: comp-research-completion-handler
type: component
status: active
created: 2026-05-04T03:35:05.736917757Z
updated: 2026-05-04T04:50:32.301793368Z
edges:
- target: comp-jam-svc-research
  type: depended_on_by
- target: comp-tempyr-mcp-client-wrapper
  type: depends_on
- target: feat-deep-research
  type: used_by
---
On research completion, reads `findings.json` and creates a Tempyr research node with stable ID, then emits `research.completed` (§4.10). Other tasks can `query-tempyr` for it; the Maestro can cite it in Picker prompts.

Skill file `~/.jam/skills/agents/research-completion-handler.md` (§9) explains its role.

Implementation note (2026-05-06): `jam-svc-research` now has the deterministic handler path behind `JAM_RESEARCH_TEMPYR_GRAPH_DIR`. After provider output lands, it reads `report.md`, `findings.json`, `sources.jsonl`, and `metadata.json`, writes a stable `note-research-<research-id>` node under the configured Tempyr graph's `notes/` directory, and publishes `journal.research.tempyr-node-created` with the node ID. `scripts/smoke-research-service.sh` exercises this path with a temporary graph and verifies the note plus journal entry. Real deep-tier acceptance still needs provider credentials to produce non-fake output.
