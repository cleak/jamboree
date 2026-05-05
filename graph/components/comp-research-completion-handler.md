---
id: comp-research-completion-handler
type: component
status: planned
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