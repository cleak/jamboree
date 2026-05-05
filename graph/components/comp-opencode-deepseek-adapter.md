---
id: comp-opencode-deepseek-adapter
type: component
status: planned
created: 2026-05-04T03:34:41.920144119Z
updated: 2026-05-04T04:42:17.033439531Z
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
OpenCode + DeepSeek V4 Pro. Open-source terminal-native harness, configured with DeepSeek V4 Pro as default model. Pay-per-use API (§4.5.3).

Why this combination: OpenCode supports 75+ providers via Models.dev; AGENTS.md project config matches Codex CLI's pattern (skill files transfer); DeepSeek V4 Pro at sale pricing ($0.435/$0.87 per 1M tokens until 2026-05-31 15:59 UTC) is 11–34x cheaper than GPT-5.5 API; at regular pricing ($1.74/$3.48 per 1M) still 3–7x cheaper. Benchmarks: 80.6% SWE-bench Verified, 93.5 LiveCodeBench, 67.9% Terminal-Bench 2.0.

Latency caveat: at max reasoning effort runs ~33 tokens/sec — verbose. Not a fit for latency-sensitive interactive work; ideal for overnight batch jobs and compile-heavy refactors where wall-clock is acceptable but cost matters.

V4 Flash ($0.14/$0.28 per 1M) routes here too, used for low-stakes background work. The Maestro can specify model per task within this harness.

OpenCode does not have first-class Tempyr SessionStart/SessionEnd hooks. The harness adapter wraps the OpenCode invocation: prefix with `tempyr journal bootstrap`, append `tempyr journal finalize` to cleanup. If the Picker is `full-stop`'d before the wrapper runs cleanup, the harness adapter's cleanup runs `tempyr journal finalize` itself.