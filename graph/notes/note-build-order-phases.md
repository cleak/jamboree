---
id: note-build-order-phases
type: note
created: 2026-05-04T03:48:26.486423028Z
updated: 2026-05-04T05:07:07.672895348Z
edges:
- target: feat-implementation-walkthrough-reference
  type: relates_to
- target: jamboree-v5
  type: relates_to
---
§12 phased plan with explicit acceptance criteria per phase. Each phase is "done" when its acceptance criteria pass on a fresh checkout.

- Phase 0 — Foundations (1-2 weeks): workspace skeleton, NATS up, journal writer, codegen pipeline, setup script, secrets backend, trace plumbing, base UI shell.
- Phase 1 — Maestro MVP + observation + Tempyr canonical worktree + session store (2-3 weeks): end-to-end one-task path. Codex CLI only. local × default profile.
- Phase 2 — Review weirdness loop (1-2 weeks): reviewer adapters, GitHub App auth, Untrusted-content discipline.
- Phase 3 — Multi-harness + dispatch (1-2 weeks): Claude Code adapter, OpenCode + DeepSeek adapter, harness version pinning, quota tracker.
- Phase 3.5 — Search and research (1 week).
- Phase 4 — Hardened sandbox + Hermes Docker backend (1-2 weeks).
- Phase 5 — Hermes skill evolution + self-improvement (2-3 weeks).
- Phase 6 — UI server (2 weeks).
- Phase 7 — Hot-patching infrastructure + patch agent (2 weeks).
- Phase 8 — MCP integration polish (1 week).
- Phase 9 — Production hardening (1-2 weeks): 7-day continuous run, runbooks, perf tuning.

Total estimate: 14-21 weeks if linear; many phases can overlap once Phase 0+1 land.