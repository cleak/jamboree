---
id: principle-self-improvement-via-markdown-git-hermes
type: constraint
status: active
created: 2026-05-04T03:23:48.302715942Z
updated: 2026-05-04T04:18:29.154179857Z
edges:
- target: feat-record-learning
  type: constrains
- target: feat-self-improvement
  type: constrains
- target: feat-skill-evolution-pipeline
  type: constrains
- target: feat-tempyr-knowledge-and-journal
  type: constrains
---
**§2.6 Self-improvement = structured markdown + git + Hermes evolution.**

Skills live as markdown files in a git repo. The Maestro reads them and writes new ones via `record-learning`. The Hermes evolution pipeline (DSPy + GEPA, vendored as a subsystem) periodically optimizes skill files against FTS5 session-store eval data and Tempyr's `dead_end` corpus.

Version control for free, human review for free, hot-editing for free, compounding optimization without writing the optimization infrastructure.

*Why:* skill markdown is both human-readable and LLM-friendly. Git is the durability and review system. Hermes' DSPy+GEPA pipeline is the optimization machinery. Adopt all three; build none of them.