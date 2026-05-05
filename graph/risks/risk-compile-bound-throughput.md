---
id: risk-compile-bound-throughput
type: risk
status: identified
created: 2026-05-04T03:46:51.497657837Z
updated: 2026-05-04T03:46:51.497658239Z
---
**§13.1 Compile-bound throughput.** Bevy compile times limit per-machine concurrent compile-heavy task throughput.

Mitigation: shared sccache, mold linker, shared `target/`. Modal/SSH backends for elastic compute when local saturates. Rare in practice — most tasks aren't full-recompile.