---
id: task-vendor-hermes-evolution
type: task
status: blocked
created: 2026-05-04T03:59:38.162154741Z
updated: 2026-05-06T17:07:36.122999766Z
edges:
- target: feat-skill-evolution-pipeline
  type: child_of
---
Phase 5 (§12). Vendor Hermes' evolution subsystem.

Per `comp-hermes-evolution-subsystem`, `feat-skill-evolution-pipeline`.

Boundary discipline: pipeline runs as subprocess. Reads a directory of skills + an eval data path, writes a diff. No Hermes module imports into main orchestrator code.

Acceptance: subprocess runs DSPy + GEPA optimization end-to-end; outputs candidate diff.

Implementation note (2026-05-06): the spec-referenced subsystem now exists as the separate official repo `NousResearch/hermes-agent-self-evolution`, not inside the main `NousResearch/hermes-agent` checkout. Jamboree vendors commit `4693c8f0eed21e39f065c6f38d98d2a403a04095` under `evolution/hermes-agent-self-evolution/` and adds `evolution/jamboree_evolve_skill.py` as the subprocess-only adapter. The adapter converts Jamboree skill markdown into a temporary Hermes-compatible skill tree, invokes `python -m evolution.skills.evolve_skill`, and writes candidate diffs under `$JAM_HOME/skills-evolution-candidates/` for human review. `scripts/smoke-hermes-evolution-vendor.sh` passed: 139 upstream tests plus a Jamboree subprocess dry-run against a temporary skill.

Blocked note (2026-05-06): full acceptance still needs a real DSPy/GEPA optimization run that writes a candidate diff. Local state has no DSPy/LiteLLM-compatible LLM credential (`OPENAI_API_KEY`, `LITELLM_API_KEY`, `ANTHROPIC_API_KEY`, and `DEEPSEEK_API_KEY` are unset; `pass show jam/maestro/openai-api-key` is missing). To finish acceptance, seed an optimizer/eval model credential for the runtime user, then run `evolution/jamboree_evolve_skill.py --skill-path <skill> --dataset-path <eval-dataset> --iterations <n>` and verify the candidate diff is written.
