---
id: note-architecture-overview
type: note
created: 2026-05-04T03:48:22.383152015Z
updated: 2026-05-04T05:06:38.776568898Z
edges:
- target: jamboree-v5
  type: relates_to
---
§3 architecture overview ASCII diagram in `docs/proposal-v5.md` lines 254-360. Rendered structure:

- Top: HUMAN OPERATOR (CLI, web UI, mobile via Tailscale, ntfy push).
- UI SERVER (`jam-ui-server`, Rust + axum) — serves SolidJS SPA, WebSocket→NATS bridge, REST endpoints for world-snapshot, journal, trace replay; ntfy push; session-token auth.
- MAESTRO (Python, episodic GPT-5.5 sessions via LiteLLM).
- TOOL SERVICES (Rust, separate processes, atomic-swappable via routing manifest): `jam-svc-observe/-session/-worktree/-repo/-knowledge/-search/-research/-message/-supervise/-evolve`.
- OBSERVATION TOOL SERVICE — world-snapshot fact compiler, compute-readiness, list-blockers, branch-staleness, quota state, review-artifact classifier.
- SUBSTRATE — NATS JetStream + KV; orch journal + session DB (JSONL + SQLite/FTS5); quota tracker; reconcilers (stall detector, journal-reconciler, task-lifecycle-handler, tempyr-pr-reconciler, trunk-fetcher, pr-status-poller, skill-suspicion); skill evolution pipeline; patch agent + supervisor (process-compose).
- PICKER LAYER — Subscription tier (Codex CLI, Claude Code), API tier (OpenCode + DeepSeek V4 Pro), Specialized (Aider, Cursor CLI, others). Each Picker: own worktree, own sandbox, own Tempyr journal session.
- EXTERNAL SUBSYSTEMS — Tempyr (knowledge graph + journal + canonical worktree + git-ref publishing), Search router (Brave/Firecrawl/Exa/Linkup/...), MCP servers (Context7, Composio, Tavily-MCP, project-specific), Reviewer adapters (CodeRabbit, codex-review, custom), Deep research providers (Tavily/Sonar Pro/Exa Deep/Parallel Pro).

Trust boundary: between Tool Services and Substrate. Above the line: Maestro judgment. Below the line: mechanical enforcement. Pickers below another boundary entirely — sandboxed processes that the Maestro talks to through harness adapters.

Three boundaries an implementer must respect: trust boundary (tool services trust Maestro; Pickers do not), process boundary (each tool service its own process), provider boundary (every external provider behind a trait).