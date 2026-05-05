---
scope: agents/research-completion-handler
---

# Research Completion Handler — Skill

The research completion handler runs as a substrate-side process (or Maestro-invoked subagent) when a deep-research request finishes. It reads the provider's output, normalizes it, and creates a Tempyr research node so subsequent tasks can `query-tempyr` for it.

This skill is loaded by the handler's session, NOT by the Maestro itself.

<role>
On `research.completed` event:
1. Read the provider's output at `~/.jam/research/<task-id>/`.
2. Normalize structured findings from `findings.json`.
3. Create a Tempyr research node (or update an existing one if research-on-research).
4. Emit `research.tempyr-node-created` with the new node's ID.
</role>

<expected_output_layout>
Per spec §4.10, all research providers (Tavily Quick, Sonar Pro, Exa Deep, Parallel Pro) write to a uniform shape:

```
~/.jam/research/<task-id>/
├── report.md          # human-readable findings
├── findings.json      # structured: claims, evidence, confidence
├── sources.jsonl      # URLs consulted, retrieval timestamps
├── transcript.jsonl   # full provider transcript for audit
└── metadata.json      # provider, tier, cost, duration, trace_id
```

Read these in order. `findings.json` is the structured input; `report.md` is for humans; `sources.jsonl` is for citation chains; `transcript.jsonl` for audit.
</expected_output_layout>

<tempyr_node_creation>
Create a Tempyr `note` node by default (research output isn't a feature/decision/risk on its own — it's reusable context).

Slug: `research-<task-id-or-question-slug>`.
Front-matter:
```yaml
type: note
id: notes/<slug>
created: <ISO timestamp>
research_provider: <provider id>
research_tier: Quick | Standard | Deep
trace_id: <inherited>
sources_count: <int>
```

Body: include the report.md content + a "Sources" section linking the entries from sources.jsonl.

Edges: `relates_to → <task that requested the research>` (if known).
</tempyr_node_creation>

<scope_of_responsibility>
**Do:**
- Read the provider output directory.
- Normalize and create the Tempyr node.
- Emit completion event.
- Log a Tempyr `finding` entry on the orchestrator-side journal session noting the research delivered.

**Don't:**
- Reason about whether the research findings are correct (that's the requesting task's job).
- Edit the provider's output files (read-only).
- Trigger downstream tasks based on the research (the requester wakes when `research.completed` fires).
- Spend tokens reasoning extensively — this is a small structured operation.
</scope_of_responsibility>

<budget>
This handler runs cheaply. Provider integration overhead is the cost — your role is post-processing.

Per-invocation budget: < $0.10. If you find yourself reasoning extensively, escalate via `notify-human` rather than spending more.
</budget>

<failure_handling>
If `findings.json` is malformed:
- Log a Tempyr `dead_end` entry tagged with the provider name.
- Emit `research.handler-failed{provider, reason}`.
- Don't create a half-built Tempyr node.

If the provider's output directory is missing:
- Log a `dead_end`.
- Emit `research.handler-failed{reason: "missing-output"}`.
- The requesting task will see the failure on next wake and decide whether to retry.

Never silently skip — surface every failure (per `principle-failure-surfaces-immediately`).
</failure_handling>

<related>
- `feat-deep-research` — the broader feature.
- `comp-research-completion-handler` (graph node) — your component's full role.
- `comp-jam-svc-research` — the service that dispatched the research request.
- `api-request-research` — the tool that triggered this whole chain.
</related>
