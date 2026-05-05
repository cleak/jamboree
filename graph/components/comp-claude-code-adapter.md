---
id: comp-claude-code-adapter
type: component
status: planned
created: 2026-05-04T03:34:41.128626236Z
updated: 2026-05-04T04:42:09.561037375Z
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
Anthropic's first-party agentic coding harness. Auth via Claude Pro/Max subscription (§4.5.2).

Strong reasoning depth; particularly good on architectural / cross-system refactors and deep code review. Supports interrupt via Esc-key. Default sandbox: `local`.

Quota mechanics: rate-limit shape per Anthropic's published docs. The April 4 2026 block on third-party harnesses does **not** apply to Claude Code itself — it's first-party.

Tempyr journal integration: Claude Code supports `SessionStart`/`SessionEnd` hooks via `.claude/settings.json`. The harness adapter writes the relevant config into the Picker's worktree before spawn.

Phase 3 add (§12.3).