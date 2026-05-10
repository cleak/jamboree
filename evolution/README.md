# Jamboree Evolution Subsystem

This directory vendors the Hermes Agent self-evolution subsystem required by
`docs/proposal-v5.md` §17.1.

Vendored source:

- Repository: `https://github.com/NousResearch/hermes-agent-self-evolution`
- Commit: `4693c8f0eed21e39f065c6f38d98d2a403a04095`
- License: MIT, as declared by the upstream `pyproject.toml`

Boundary discipline from §17.1 and §2.9 still applies: Jamboree does not import
Hermes modules into the Maestro or Rust services. The coordinator invokes the
pipeline as a subprocess. The Jamboree adapter in `jamboree_evolve_skill.py`
turns Jamboree skill markdown into the Hermes-compatible temporary layout,
runs `python -m evolution.skills.evolve_skill`, and writes a candidate diff to
`$JAM_HOME/skills-evolution-candidates/` for human review.

Local validation:

```bash
scripts/smoke-hermes-evolution-vendor.sh
```

The smoke installs the vendored Python package in an isolated `uv` environment,
runs upstream tests, and performs a dry-run subprocess call against a temporary
Jamboree-style skill. A real optimization run still needs an LLM API key
accepted by DSPy/LiteLLM, and costs upstream estimate at roughly $2-10 per
optimization run.
