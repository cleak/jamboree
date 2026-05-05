---
id: feat-picker-layer-three-tier
type: feature
status: draft
created: 2026-05-04T03:28:17.283130790Z
updated: 2026-05-04T05:56:11.097733972Z
owner: caleb
edges:
- target: api-harness-adapter-contract
  type: exposes
- target: comp-aider-adapter
  type: uses
- target: comp-blueberry-brp-server
  type: uses
- target: comp-blueberry-wslg-runtime
  type: uses
- target: comp-claude-code-adapter
  type: uses
- target: comp-codex-cli-adapter
  type: uses
- target: comp-cursor-cli-adapter
  type: uses
- target: comp-harness-adapter-trait
  type: uses
- target: comp-harness-version-lockfile
  type: uses
- target: comp-opencode-deepseek-adapter
  type: uses
- target: dec-adopt-blueberry-conventions
  type: depends_on
- target: jamboree-v5
  type: child_of
- target: principle-failure-surfaces-immediately
  type: constrained_by
- target: principle-provider-agnostic-everywhere
  type: constrained_by
- target: principle-sandbox-blast-radius-not-behavior
  type: constrained_by
- target: principle-subscription-friendly-api-when-necessary
  type: constrained_by
- target: principle-tracing-chains-end-to-end
  type: constrained_by
- target: task-claude-code-adapter-impl
  type: parent_of
- target: task-harness-version-pinning
  type: parent_of
- target: task-jam-svc-session-codex-cli-only
  type: parent_of
- target: task-opencode-deepseek-adapter-impl
  type: parent_of
---
Three tiers of harness, plus specialized one-offs (§4.5):

- **Subscription tier:** Codex CLI (ChatGPT Pro $100/$200 with 5x/20x multipliers; `local-messages`/`cloud-tasks`/`code-reviews` 5h rolling windows; speed-mode burns credits faster); Claude Code (Pro/Max).
- **API tier:** OpenCode + DeepSeek V4 Pro (BYOK, ~3-7x cheaper than subscription harnesses' API-equivalent at regular pricing; 11-34x cheaper at sale pricing until 2026-05-31). V4 Flash for low-stakes background work.
- **Specialized:** Aider, Cursor CLI, others — loaded conditionally per project.

Every harness implements `HarnessAdapter` (§4.5.1) — same lifecycle, messaging, Tempyr journal, quota, version-pinning shape.

Each Picker: own worktree, own sandbox, own Tempyr journal session, own message FIFO.