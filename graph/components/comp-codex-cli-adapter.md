---
id: comp-codex-cli-adapter
type: component
status: planned
created: 2026-05-04T03:34:40.347974886Z
updated: 2026-05-04T04:42:01.201271570Z
edges:
- target: comp-harness-adapter-trait
  type: depends_on
- target: feat-picker-layer-three-tier
  type: used_by
- target: principle-provider-agnostic-everywhere
  type: constrained_by
- target: principle-subscription-friendly-api-when-necessary
  type: constrained_by
---
OpenAI's first-party agentic coding harness. Auth via ChatGPT subscription Pro $100 (5x Plus) / Pro $200 (20x Plus). Built-in worktrees, parallel project execution, Skills, Automations (§4.5.2).

Supports interrupt cleanly; supports session resume via Codex's internal session mechanism. Default sandbox: `local`; migration to `docker` is supported.

Quota mechanics: 5h rolling windows for `local-messages` (interactive), `cloud-tasks` (delegated background work), `code-reviews` (PR review). Speed-mode burns credits faster — disabled by default, enabled per-task by skill files when latency matters more than rate-limit-headroom.

Tempyr journal integration: Codex CLI supports `SessionStart`/`SessionEnd` hook integration. The harness adapter's `bootstrap_tempyr_journal` configures Codex to invoke `tempyr journal bootstrap` on SessionStart and `tempyr journal finalize` on SessionEnd.

Per memory: Codex OAuth is the auth mechanism for the Maestro itself too — same credential covers Codex-based Pickers.

Phase 1 first-harness pick (§12, §24.9 step 8): simplest because of clean Skills/SessionStart hooks.