---
scope: task-types/doc-generation
---

# Task Type — Documentation Generation

Tasks that author or update markdown documentation under `/home/caleb/blueberry/docs/`. Low-stakes, high-volume, latency-tolerant — fits API-tier harnesses well.

<concurrency_cap>
8 concurrent globally (shares the light-edit / shader-variant slot per spec §6.7). Many doc tasks can run in parallel.
</concurrency_cap>

<harness_selection>
**OpenCode + DeepSeek V4 Flash** is the default for routine doc generation:
- Cheap ($0.14/$0.28 per 1M tokens).
- Latency-tolerant.
- Doc generation rarely needs deep reasoning.

**OpenCode + DeepSeek V4 Pro** for higher-stakes docs (architecture references, runbooks, decision records).

**Claude Code** for substantial new architecture docs where structure and clarity matter.

Avoid Codex CLI for routine doc work — burns subscription quota for marginal benefit.
</harness_selection>

<sandbox_profile>
`default × local`. Doc work doesn't need hardening.
</sandbox_profile>

<doc_taxonomy>
Per Blueberry's `CLAUDE.md` (Project Structure section), docs live at:
- `docs/agents/` — agent-facing operational references and examples.
- `docs/architecture/` — current technical design and rendering reference docs.
- `docs/operations/` — CI, profiling, and runbooks.
- `docs/product/`, `docs/research/`, `docs/archive/`, `docs/working/` — active planning, exploration, history, tracked working docs.
- `docs/temp/` — temporary working documents (gitignored scratch only; never source of truth).

Pickers must check `docs/README.md` BEFORE creating or moving documentation.

For multi-session plans, audits, migration checklists: `docs/working/` (NOT `docs/temp/`).
</doc_taxonomy>

<spawn_template>
```
spawn-picker(spec={
    task_id: "...",
    harness: "opencode",
    sandbox_backend: "local",
    sandbox_profile: "default",
    task_class: "doc-generation",
    initial_prompt: """
        Doc task: <description>

        Project: Blueberry.
        Target location: docs/<area>/<filename>.md (per docs/README.md taxonomy).

        Conventions:
        - GitHub-flavored markdown.
        - Code blocks with language tags.
        - No low-value comments; explain why, not what.
        - For technical docs, reference existing files relatively.

        Acceptance:
        - File at correct location per docs/README.md.
        - Markdown lint passes.
        - Linked from relevant index doc if applicable.
        - PR opened ready-for-review.
    """,
    model_override: "deepseek-v4-flash",  # or v4-pro for high-stakes docs
    budget_usd: 0.50 - 3.00,
})
```
</spawn_template>

<reviewer_handling>
CodeRabbit on doc PRs is usually low-signal. Default action: accept formatting/grammar suggestions; reply-with-rationale on style preferences (Blueberry's `code-conventions.md` says no low-value comments).
</reviewer_handling>

<related>
- `harnesses/opencode-deepseek.md` — primary harness for this task type.
- `projects/blueberry/code-conventions.md` — comment / docstring style.
- `projects/blueberry/overview.md` — `docs/README.md` reference.
</related>
