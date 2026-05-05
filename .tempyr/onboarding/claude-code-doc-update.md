# Claude Code Tempyr Doc Update

Merge the Tempyr guidance into these existing instruction files without removing project-specific content.

Suggested Claude Code launch pattern:
- Keep Tempyr in a project-level `.mcp.json` at the repo root so the MCP config is shared and follows Git worktrees.
- Prefer relative paths in that `.mcp.json`; keep user-level `~/.claude.json` entries for personal servers, not worktree-local Tempyr config.
- For hosted embedding keys shared across worktrees, prefer Tempyr's shared Git-common-dir env file at `<git-common-dir>/tempyr/.env.local`; Tempyr loads it automatically.
- Keep shared `.mcp.json` portable: use repo-relative paths and put machine-specific Tempyr binary paths or extra launch flags in user-local MCP config or per-worktree documentation.
- Add `.env` and `.env.local` to `.worktreeinclude` when Tempyr needs provider credentials inside Claude-created worktrees, without committing machine-local overrides to shared `.mcp.json`.
- Use `--permission-mode acceptEdits`.
- Prefer `--allowedTools Read,Grep,Glob,Edit(/CLAUDE.md),Edit(/AGENTS.md)` to narrow writes to the instruction docs you want merged.
- Keep approval choices local instead of checking shared allowlists into the repo.
- Supporting `.claude` hooks/skills were installed directly by Tempyr because protected directories can still prompt.

## CLAUDE.md

```md
## Tempyr Knowledge Graph

This repository uses Tempyr, a file-based knowledge graph for product and technical design.

## Graph Location

- Graph nodes: `graph/<type>/*.md` (Markdown files with YAML frontmatter, e.g. `graph/features/feat-session-replay.md`)
- Schema: `.tempyr/schema.toml`
- Config: `.tempyr/config.toml`
- Render templates: `.tempyr/render/`
- Interview sessions: `.tempyr/sessions/`

## Preferred Workflow

- Prefer Tempyr MCP tools when they are available instead of editing graph files directly.
- Use the interview flow for new features, epics, and multi-node changes.
- Use direct file edits only when Tempyr tools are unavailable or insufficient.

## MCP Tools

When the Tempyr MCP server is running, prefer these tools:

- `graph_search` / `graph_vsearch` / `graph_context` to discover relevant nodes
- `graph_get_node` to read a node in full
- `graph_add_node` / `graph_add_edge` to create graph content
- `graph_update_node` to update status, body, or metadata on existing nodes
- `graph_traverse` to follow graph relationships
- `graph_validate` to check graph consistency after changes
- `graph_render` to generate PRDs, TDDs, or other views
- `graph_ask` to answer questions grounded in graph context
- `interview_start` / `interview_answer` / `interview_commit` for guided creation

## Rules

1. Never rename node IDs manually. Use `tempyr rename`.
2. Use human-readable kebab-case slugs when creating node IDs manually.
3. Store edges bidirectionally in YAML frontmatter, and keep each edge list alphabetized by target.
4. Run `tempyr validate` after manual graph edits.
5. Prefer updating existing nodes over creating near-duplicates.
6. If a change affects retrieval quality, rebuild or update the index.

## Environment

- Embedding provider settings live in `.tempyr/config.toml`
- API keys are typically loaded from Tempyr's shared Git-common-dir env (`tempyr/.env.local`), `.env.local`, `.env`, or the shell environment
- Repo-local `.env.local` overrides shared worktree defaults when both are present
- At each location, Tempyr loads `.env.local` before `.env`
```

