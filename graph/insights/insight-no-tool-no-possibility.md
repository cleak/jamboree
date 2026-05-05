---
id: insight-no-tool-no-possibility
type: insight
created: 2026-05-04T03:48:20.338528205Z
updated: 2026-05-04T05:06:28.984073740Z
edges:
- target: feat-maestro-tool-surface
  type: informs
- target: insight-untrusted-newtype-prevents-injection
  type: relates_to
---
**The "no tool, no possibility" rule** (§2.3).

Want to disallow a behavior? Don't put a tool there.
Want to enforce an invariant? Build it into the tool's contract.

Concrete examples:
- No `merge-pr` → Maestro cannot merge (only path is `request-human-merge`).
- No `read-file`/`write-file`/`run-command` for the Maestro → Maestro doesn't directly touch disk; Pickers do file ops in their worktrees.
- No `eval`/`exec`/`python -c` → banned at lint level.
- No `auto-rebase`/`auto-merge`/`auto-update-tempyr-node` → never auto-mutate state; always candidate queues.
- No `fork-Maestro`/`clone-session` → episodic sessions only.

A creative agent can bypass policy checks; a creative agent cannot summon a tool that doesn't exist.