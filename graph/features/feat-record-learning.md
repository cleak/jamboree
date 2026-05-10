---
id: feat-record-learning
type: feature
status: draft
created: 2026-05-04T03:28:25.751391671Z
updated: 2026-05-04T04:25:09.224442398Z
owner: caleb
edges:
- target: comp-jam-svc-knowledge
  type: uses
- target: comp-tempyr-mcp-client-wrapper
  type: uses
- target: jamboree-v5
  type: child_of
- target: principle-self-improvement-via-markdown-git-hermes
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
---
The `record-learning` tool writes a structured skill note as markdown into `~/.jam/skills/` AND emits a Tempyr `decision` (or `finding`) entry tagged with the skill scope and the new skill's path (§7.1, §22.7).

The double-write makes "why does this skill exist" trace-replayable from Tempyr's journal even after the skill itself is hot-edited or deleted.

Required fields: `scope`, `confidence`, `evidence`, `guidance`, `originated-from-trace`. Optional: `counterexample`. Skill files are markdown with structured front-matter; live in `~/.jam/skills/`; version-controlled; read by the Maestro at session start when relevant to the task scope.

`originated-from-trace` lets us trace back from a skill to the failure or finding that produced it (§23).

Implementation note (2026-05-06): the first Maestro-side implementation is active in `jam_maestro.record_learning`. It writes markdown skills to the configured skills root and logs the paired Tempyr decision through `TempyrJournalClient`, with bidirectional traceability via skill-file path and journal tags.
