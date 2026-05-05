---
id: comp-monorepo-tree
type: component
status: active
created: 2026-05-04T03:40:06.803600750Z
updated: 2026-05-04T05:04:27.559573802Z
edges:
- target: comp-source-vs-runtime-bridge
  type: depended_on_by
- target: dec-monorepo
  type: has_decision
- target: feat-monorepo-layout
  type: used_by
---
Top-level layout per layout.md:

```
/home/caleb/jamboree/
├── CLAUDE.md
├── README.md (TBD)
├── docs/
│   ├── proposal-v5.md        # Architecture spec (§0–§24)
│   ├── security-setup.md     # Multi-user isolation addendum
│   └── layout.md             # Repo layout decision
├── scripts/                  # Bootstrap and ops scripts
│   ├── bootstrap-users.sh
│   ├── install-cli-tools.sh
│   ├── cli-tools-update.sh
│   ├── init-maestro-keyring.sh
│   └── seed-maestro-secrets.sh
├── crates/                   # Rust workspace (Phase 3+)
│   ├── jam-cli/
│   ├── jam-svc-observe/
│   ├── jam-svc-supervise/
│   ├── jam-stall-detector/
│   ├── jam-ui-server/
│   └── ... (full set per spec §4)
├── maestro/                  # Python Maestro package (Phase 3+)
│   ├── pyproject.toml
│   └── src/jam_maestro/
├── ui/                       # SolidJS UI (Phase 3+)
│   ├── package.json
│   └── src/
└── Cargo.toml                # Rust workspace root (Phase 3+)
```

Lowercase for code identifiers and paths (`jam_maestro` Python pkg, `maestro/` dir, `maestro.toml` config, `crates/jam-svc-*/`). Capitalized prose: **the Maestro**, **the Pickers**, **the Manager**.