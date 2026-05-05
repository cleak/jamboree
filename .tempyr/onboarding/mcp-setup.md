# Tempyr MCP Setup

Register a stdio MCP server named `tempyr` that runs:

```text
tempyr --mcp
```

## Claude Code

- Prefer a project-level `.mcp.json` in the repo root so the MCP config is shared and follows each Git worktree.
- Use relative paths in project `.mcp.json` entries. Anthropic documents relative paths for project-scoped `.mcp.json` and absolute paths for user-level `~/.claude.json`.
- For hosted embedding keys shared across worktrees, prefer Tempyr's shared Git-common-dir env file at `<git-common-dir>/tempyr/.env.local`. Tempyr loads that automatically without committing it.
- If `tempyr` is already on `PATH`, use a minimal project config like:

```json
{
  "mcpServers": {
    "tempyr": {
      "command": "tempyr",
      "args": ["--mcp"],
      "env": {}
    }
  }
}
```

- Keep repo-root `.mcp.json` portable: use repo-relative paths and avoid committing machine-specific Tempyr binary paths or extra launch flags. Put those overrides in user-local MCP config or per-worktree documentation instead.
- If Tempyr needs repo-local `.env` or `.env.local` files, add them to `.worktreeinclude` so Claude-created worktrees copy those gitignored files without committing machine-local overrides to shared `.mcp.json`.
- Keep Claude approval choices and other machine-specific trust settings local instead of checking them into the repo.
- If you want Claude to merge existing instruction docs, prefer `--permission-mode acceptEdits` with narrow `Edit(...)` tool rules for the target markdown files.

## Codex

- Prefer a project-scoped `.codex/config.toml` entry instead of a user-level `~/.codex/config.toml` entry when you want MCP to follow Git worktrees.
- Do NOT set the `cwd` field in the project config. Codex Desktop launches MCP servers in the workspace directory by default, which is what Tempyr's project-root walk-up needs. A relative parent-directory `cwd` triggers a Codex Desktop bug where the path resolves against the desktop process cwd (e.g. `C:\WINDOWS\system32` on Windows), stranding Tempyr in `C:\WINDOWS`.
- For hosted embedding keys shared across worktrees, prefer Tempyr's shared Git-common-dir env file at `<git-common-dir>/tempyr/.env.local`. Tempyr loads that automatically without committing it.
- Example:

```toml
[mcp_servers.tempyr]
command = "tempyr"
args = ["--mcp"]
startup_timeout_sec = 5
```

- If your client doesn't launch MCP servers in the workspace directory, keep any absolute `--project-root` fallback in local user or machine config, not in checked-in project config.
- Avoid absolute MCP `cwd`, `--project-root`, or `TEMPYR_PROJECT_ROOT` values in shared config if you want the same setup to follow Git worktrees cleanly.
- Use `TEMPYR_GRAPH_DIR` only when you need to anchor directly to a nonstandard graph path.
- If you want Codex to update existing instruction docs, use project config with narrow writable roots for those markdown files.
- Repo-local `.codex` and `.agents` paths can remain protected even when writable roots are restricted, so Tempyr installs supporting assets directly and limits merge handoffs to markdown docs.
