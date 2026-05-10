---
id: task-document-blueberry-manual-onboarding
type: task
status: done
created: 2026-05-04T05:38:18.142068285Z
updated: 2026-05-06T05:49:51Z
edges:
- target: dec-manual-project-onboarding-v1
  type: blocked_by
- target: feat-jam-cli
  type: child_of
- target: task-jam-project-add-future
  type: blocks
---
Document the manual Blueberry project onboarding steps. Probably in `docs/onboard-blueberry.md`.

Per `dec-manual-project-onboarding-v1`. Cover:
1. Run `bootstrap-users.sh` (and `install-cli-tools.sh` and `seed-maestro-secrets.sh`).
2. Create `~/.jam/config/projects/blueberry.toml` with documented fields (trunk-branch, fetch-staleness-secs, canonical-worktree path, harnesses, max-concurrent, etc.).
3. Create `~/.jam/config/projects/blueberry-harnesses.lock`.
4. Create the canonical Tempyr worktree at `~/code/blueberry-tempyr-live/`.
5. Initialize the skills repo at `~caleb/code/jam-skills/` (Phase 1 has Maestro.md + minimal scaffolding).
6. Verify with `jam doctor`.

Acceptance: a fresh Blueberry onboarding from a clean machine completes in <30min following only the doc.

Implementation note (2026-05-06): Added `docs/onboard-blueberry.md` with the manual v1 flow: bootstrap scripts, substrate install, `blueberry.toml` template, `blueberry-harnesses.lock` template, canonical worktree creation/recovery using `/home/caleb/blueberry-jam` per `dec-blueberry-jam-path`, skills verification, process-compose startup, `jam doctor`, and NATS-to-JSONL smoke checks.
