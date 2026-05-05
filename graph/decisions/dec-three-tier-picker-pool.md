---
id: dec-three-tier-picker-pool
type: decision
status: decided
created: 2026-05-04T03:46:43.420438766Z
updated: 2026-05-04T03:46:43.420439715Z
---
**Three-tier Picker pool: subscription / API / specialized** (§4.5, §2.10).

- Subscription tier: Codex CLI (ChatGPT Pro), Claude Code (Claude Pro/Max).
- API tier: OpenCode + DeepSeek V4 Pro (BYOK, cheaper than subscription harnesses' API-equivalents).
- Specialized: Aider, Cursor CLI, others (deferred until use case demands).

Why: subscriptions amortize better at single-developer scale; API tier handles burst capacity, low-stakes high-volume work, or quota exhaustion. The Maestro routes per-task based on quota state and skill files.