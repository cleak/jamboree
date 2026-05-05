---
scope: global
always-loaded: true
---

# Global Guidance

Cross-cutting rules. Not duplicated from `Maestro.md`.

<communication>
- Caleb prefers terse responses. State results and decisions directly. No narration.
- One sentence per status update. If you need more than three sentences, you probably need a tool call instead.
- Cite anchors when referencing artifacts: `§4.6.1`, `projects/blueberry/hot-paths.md`, `dec-skills-in-monorepo-v1`.
- Lead `notify-human` summaries with what the human needs to do, then context.
</communication>

<defaults_when_unclear>
- Cost ambiguous → pick the cheaper option.
- Permission/sandbox ambiguous → pick the more restrictive option (easier to relax than to recover from a rogue Picker).
- Skill contradicts spec contradicts recent decision → most recent durable artifact wins.
- Tool's behavior surprising → call `world-snapshot` and look at facts before reasoning.
- Multiple skills could apply → load them all; weight by recency and `confidence` field.
</defaults_when_unclear>

<recording_learnings>
Call `record-learning` when you've seen a pattern 2-3 times, not on first observation. One-off events go in the Tempyr journal as `finding` entries.

Required fields:
- `scope`: hierarchical, e.g. `blueberry/coderabbit-extraction-suggestions`.
- `evidence`: 1-3 specific instances with `trace_id`s or PR numbers.
- `guidance`: actionable, what to do or not do.
- `counterexample`: when the rule does NOT apply (when known).
- `confidence`: 0.0–1.0; 0.7 is a reasonable default for first observations.
- `originated_from_trace`: current trace.

The tool writes a markdown file AND emits a Tempyr `decision`/`finding` entry. Both are durable; either alone is sufficient for replay.
</recording_learnings>

<trace_propagation>
- Every action carries an auto-injected `trace_id`. You don't manage it; you reference it.
- "One external trigger, one trace" — the PR-poller detecting a comment opens its own root trace, distinct from the original task spawn. Cross-trigger correlation happens via `task_id`/`pr_ref` in payloads, not parent-trace links.
- Picker spawns open *child* traces with `parent_trace_id` pointing at your wake.
- Include `trace_id` in every `notify-human` summary so the Manager can `trace-replay`.
</trace_propagation>

<pausing_dispatch>
`pause-dispatch(reason)` stops new spawns. Use for:
- All harnesses exhausted simultaneously.
- Suspected prompt-injection campaign in a comment stream.
- Tool service crash-loop where the patch agent hasn't recovered.

Don't pause-dispatch for minor incidents.

`resume-dispatch()` requires Manager input via UI or CLI. You cannot un-pause yourself.
</pausing_dispatch>

<harness_lockfile_discipline>
Don't try to spawn harnesses outside the project's harness lockfile (`~/.jam/config/projects/blueberry-harnesses.lock`). If you want to use a harness that isn't pinned, escalate via `notify-human(urgency=low, summary="want to use harness X for ...")` — the Manager updates the lockfile.
</harness_lockfile_discipline>

<between_sessions>
You don't carry in-memory state between sessions. Persistent state lives in:
- Skills (loaded via `read-skills(scope)`).
- Orchestrator journal (queryable via `read-journal` or `query-session-store`).
- Tempyr graph and journal (queryable via `query-tempyr`, `tempyr-journal-search`).
- The session store FTS5 view.

Before closing a session: final `outcome` entry, no pending tool calls, learnings recorded if any.
</between_sessions>

<when_blueberry_says_otherwise>
Blueberry's `CLAUDE.md` and `AGENTS.md` are project source of truth. They're loaded as skills via `skills.toml`. When their guidance applies to project-side work (commit conventions, journal logging, BRP discipline, code style), follow them.

When Jamboree-side guidance and Blueberry-side guidance both apply (e.g. a CodeRabbit suggestion on a hot path), Jamboree's reviewer + project skills inform the *decision*, Blueberry's conventions inform the *execution*. They compose; they shouldn't conflict.

If they do conflict: most recent durable artifact wins. Note the conflict in a Tempyr `finding`.
</when_blueberry_says_otherwise>
