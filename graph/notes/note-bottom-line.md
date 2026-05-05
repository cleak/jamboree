---
id: note-bottom-line
type: note
created: 2026-05-04T03:48:24.429713835Z
updated: 2026-05-04T05:06:48.480237889Z
edges:
- target: jamboree-v5
  type: relates_to
---
**§16 Bottom line.** The orchestrator runs many sandboxed coding-agent Pickers in parallel, with a small Python Maestro making decisions from a typed "current truth" view (`world-snapshot`) compiled by an out-of-process Rust observation service. Tool services are separate processes, atomically swappable for hot-patches under a patch-agent supervisor.

State lives in three places:
1. An append-only JSONL journal (orchestrator events).
2. A Tempyr knowledge graph + journal (durable knowledge and agent reasoning).
3. An FTS5-indexed session store (derived view for queries).

All connected by a NATS JetStream bus carrying trace IDs through every message, so any failure is reconstructible from durable storage.

Pickers are pinned by harness version, run in pristine worktrees branched from `origin/<trunk>`, journal their reasoning to Tempyr from their own worktree. The Maestro reasons from a canonical Tempyr worktree separate from the user's pristine main checkout.

Subscriptions cover routine work; API tier (DeepSeek V4 Pro) handles burst. Linux-only; WSL native FS only. Failures fail loudly. Traces never break.