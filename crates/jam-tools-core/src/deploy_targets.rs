//! Shared registry of deploy targets (Rust binaries that can be hot-patched
//! by `jam-patch-agent`).
//!
//! Each entry maps a short CLI-facing name (e.g. `worktree`, `ui-server`) to:
//! - the cargo crate that produces it (`jam-svc-worktree`, `jam-ui-server`)
//! - the built binary filename (same as the crate name in practice)
//! - the service identifier the binary reports in its health-check response
//!
//! Tool services follow `jam-svc-<name>`; single-purpose runners
//! (`jam-ui-server`, `jam-pr-poller`, `jam-nats-bridge`, `jam-task-lifecycle`)
//! drop the `svc-` infix per `CLAUDE.md`'s "prefix where the namespace is
//! shared with the OS; drop it where the namespace is already `jam`" rule.
//!
//! Adding a new patchable binary = one entry below. Both `jam-cli` (the
//! deploy driver) and `jam-patch-agent` (the runtime swap loop) read this
//! list, so there is one source of truth for naming conventions and no
//! caller has to special-case the `jam-svc-` vs `jam-` boundary.

/// How a deploy actually swaps the running binary. Tool services answer typed
/// RPCs over versioned NATS subjects so two versions can run side-by-side
/// during health-check validation. Singletons that hold exclusive resources
/// (HTTP ports, file locks) cannot; they need a stop-replace-restart path.
///
/// Adding a strategy: extend the enum, dispatch in `jam-patch-agent`'s
/// `apply_staged_patch` and `jam-cli`'s `run_deploy_inner`. Targets opt in by
/// referencing the variant in their `DeployTarget::strategy` field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeployStrategy {
    /// `§20.3` atomic swap. Patch-agent stages the candidate on a versioned
    /// NATS subject, gates on ping + smoke, then atomically swaps the routing
    /// manifest. Two versions run concurrently for the duration of the gate.
    /// Suitable for tool services that take RPCs over `tool.<name>.*`.
    AtomicSwap,
    /// Stop the named process-compose service, atomically replace the binary
    /// at `~maestro/.jam/bin/<binary_name>` (no `-<version>` suffix), then
    /// start it back up. Patch-agent emits `patch.confirmed` once the
    /// restarted process is back to `Running`. Used for singletons:
    /// HTTP servers, file-locking reconcilers, etc.
    StopReplaceRestart {
        /// Name in `process-compose.yaml`. E.g. `"ui-server"`.
        process_name: &'static str,
    },
    /// Python virtualenv app: rsync source files into `install_dir`,
    /// refresh the venv with `uv pip install`, then restart the named
    /// process-compose service. Used for `maestro` (the orchestrator).
    /// Source isn't a single binary, so `binary_name` is informational
    /// only; staging path points at the workspace's source root (e.g.
    /// `/home/caleb/jamboree/maestro`).
    PythonApp {
        /// Where the venv + installed source live. E.g. `/opt/jam/maestro`.
        install_dir: &'static str,
        /// Name in `process-compose.yaml`. E.g. `"maestro"`.
        process_name: &'static str,
    },
    /// Install a binary directly into a canonical root-owned location (no
    /// versioned suffix, no process restart). Used for the `jam` CLI which
    /// is not a long-running process. Patch-agent renames atomically over
    /// the destination; subsequent CLI invocations pick up the new binary.
    CanonicalBinary {
        /// Absolute destination path. E.g. `/opt/jam/bin/jam`.
        dest_path: &'static str,
    },
}

/// One patchable binary's identity. Lifetimes are `'static` because the
/// registry is a const slice — runtime registries (config-loaded) would
/// use `String` and live behind an `Arc`.
#[derive(Debug, Clone, Copy)]
pub struct DeployTarget {
    /// Short, CLI-facing name. E.g. `worktree`, `ui-server`.
    pub short_name: &'static str,
    /// Cargo package name (`Cargo.toml [package] name`). Used by
    /// `cargo build --release -p <crate_name>`.
    pub crate_name: &'static str,
    /// Built binary filename (matches `crate_name` for first-party binaries).
    /// Used by `jam-patch-agent` when computing the runtime path
    /// (`~maestro/.jam/bin/<binary_name>-<version>` for AtomicSwap,
    /// `~maestro/.jam/bin/<binary_name>` for StopReplaceRestart).
    pub binary_name: &'static str,
    /// String the running service reports as its `service` field in
    /// `tool.<name>.ping` health responses. Used to validate that a candidate
    /// process matches the expected identity before swapping the routing
    /// manifest. Defaults to `binary_name`. Unused for non-AtomicSwap targets.
    pub service_id: &'static str,
    /// How patch-agent should install this binary into runtime.
    pub strategy: DeployStrategy,
}

/// Single source of truth for patchable Rust binaries.
///
/// New entries should only need this one constant updated; downstream
/// consumers look up via [`find`] or [`binary_name`].
pub const DEPLOY_TARGETS: &[DeployTarget] = &[
    // Tool services (jam-svc-* — answer typed RPCs over tool.<name>.* subjects).
    // These use AtomicSwap because two versions can coexist on different
    // subject prefixes during the validation gate.
    DeployTarget {
        short_name: "message",
        crate_name: "jam-svc-message",
        binary_name: "jam-svc-message",
        service_id: "jam-svc-message",
        strategy: DeployStrategy::AtomicSwap,
    },
    DeployTarget {
        short_name: "observe",
        crate_name: "jam-svc-observe",
        binary_name: "jam-svc-observe",
        service_id: "jam-svc-observe",
        strategy: DeployStrategy::AtomicSwap,
    },
    DeployTarget {
        short_name: "repo",
        crate_name: "jam-svc-repo",
        binary_name: "jam-svc-repo",
        service_id: "jam-svc-repo",
        strategy: DeployStrategy::AtomicSwap,
    },
    DeployTarget {
        short_name: "session",
        crate_name: "jam-svc-session",
        binary_name: "jam-svc-session",
        service_id: "jam-svc-session",
        strategy: DeployStrategy::AtomicSwap,
    },
    DeployTarget {
        short_name: "supervise",
        crate_name: "jam-svc-supervise",
        binary_name: "jam-svc-supervise",
        service_id: "jam-svc-supervise",
        strategy: DeployStrategy::AtomicSwap,
    },
    DeployTarget {
        short_name: "worktree",
        crate_name: "jam-svc-worktree",
        binary_name: "jam-svc-worktree",
        service_id: "jam-svc-worktree",
        strategy: DeployStrategy::AtomicSwap,
    },
    // Single-purpose runners (no `svc-` infix). nats-bridge, pr-poller, and
    // task-lifecycle are pure NATS subscribers — no exclusive port — so they
    // could in principle use AtomicSwap, but they're singletons with no
    // versioned subject namespace today. Use StopReplaceRestart until we add
    // versioned subject support (or until the §20.3 path is generalized).
    DeployTarget {
        short_name: "nats-bridge",
        crate_name: "jam-nats-bridge",
        binary_name: "jam-nats-bridge",
        service_id: "jam-nats-bridge",
        strategy: DeployStrategy::StopReplaceRestart {
            process_name: "jam-nats-bridge",
        },
    },
    DeployTarget {
        short_name: "pr-poller",
        crate_name: "jam-pr-poller",
        binary_name: "jam-pr-poller",
        service_id: "jam-pr-poller",
        strategy: DeployStrategy::StopReplaceRestart {
            process_name: "pr-status-poller",
        },
    },
    DeployTarget {
        short_name: "task-lifecycle",
        crate_name: "jam-task-lifecycle",
        binary_name: "jam-task-lifecycle",
        service_id: "jam-task-lifecycle",
        strategy: DeployStrategy::StopReplaceRestart {
            process_name: "task-lifecycle-handler",
        },
    },
    // ui-server binds HTTP port 8787 — must be StopReplaceRestart since two
    // candidates can't both hold the port.
    DeployTarget {
        short_name: "ui-server",
        crate_name: "jam-ui-server",
        binary_name: "jam-ui-server",
        service_id: "jam-ui-server",
        strategy: DeployStrategy::StopReplaceRestart {
            process_name: "ui-server",
        },
    },
    // Maestro orchestrator (Python). `crate_name` is the cargo-style key the
    // CLI accepts; there's no actual cargo crate. `binary_name` is informational.
    DeployTarget {
        short_name: "maestro",
        crate_name: "jam-maestro",
        binary_name: "jam-maestro",
        service_id: "jam-maestro",
        strategy: DeployStrategy::PythonApp {
            install_dir: "/opt/jam/maestro",
            process_name: "maestro",
        },
    },
    // The `jam` CLI itself. Self-update target — drops a new binary at
    // `~maestro/.jam/bin/jam`. install-substrate.sh creates a stable
    // `/usr/local/bin/jam` symlink pointing at that path during one-time
    // install (and a back-compat `/opt/jam/bin/jam` symlink for older
    // callers). Writing to maestro's home means patch-agent (which runs
    // as maestro post-systemd-switch) can self-update without sudo or
    // the jam-install-bin wrapper.
    DeployTarget {
        short_name: "cli",
        crate_name: "jam-cli",
        binary_name: "jam",
        service_id: "jam-cli",
        strategy: DeployStrategy::CanonicalBinary {
            dest_path: "/home/maestro/.jam/bin/jam",
        },
    },
];

/// Look up the registry entry for a short service name. Returns `None` for
/// unknown names; callers should error with a remediation hint that points at
/// this file.
pub fn find(short_name: &str) -> Option<&'static DeployTarget> {
    DEPLOY_TARGETS.iter().find(|t| t.short_name == short_name)
}

/// Convenience: the binary filename for a known short name. Returns `None`
/// rather than guessing for unknown names — silent fallback would let typos
/// stage a "jam-svc-typo" binary that no service can ever match.
pub fn binary_name(short_name: &str) -> Option<&'static str> {
    find(short_name).map(|t| t.binary_name)
}

/// Convenience: cargo crate name for a known short name.
pub fn crate_name(short_name: &str) -> Option<&'static str> {
    find(short_name).map(|t| t.crate_name)
}

/// Convenience: health-check service identifier for a known short name.
pub fn service_id(short_name: &str) -> Option<&'static str> {
    find(short_name).map(|t| t.service_id)
}

/// Every known short name, in registration order. Useful for `--all` flows
/// and dirty-path inference fallbacks.
pub fn all_short_names() -> impl Iterator<Item = &'static str> {
    DEPLOY_TARGETS.iter().map(|t| t.short_name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_duplicate_short_names() {
        let mut seen = std::collections::HashSet::new();
        for target in DEPLOY_TARGETS {
            assert!(
                seen.insert(target.short_name),
                "duplicate short_name in DEPLOY_TARGETS: {}",
                target.short_name
            );
        }
    }

    #[test]
    fn find_returns_expected_target() {
        let target = find("ui-server").unwrap();
        assert_eq!(target.crate_name, "jam-ui-server");
        assert_eq!(target.binary_name, "jam-ui-server");
    }

    #[test]
    fn legacy_svc_services_keep_jam_svc_naming() {
        let target = find("worktree").unwrap();
        assert_eq!(target.binary_name, "jam-svc-worktree");
        assert_eq!(target.service_id, "jam-svc-worktree");
    }

    #[test]
    fn tool_services_use_atomic_swap() {
        for short in [
            "message",
            "observe",
            "repo",
            "session",
            "supervise",
            "worktree",
        ] {
            let target = find(short).unwrap();
            assert_eq!(
                target.strategy,
                DeployStrategy::AtomicSwap,
                "{short} should use AtomicSwap"
            );
        }
    }

    #[test]
    fn ui_server_uses_stop_replace_restart() {
        let target = find("ui-server").unwrap();
        let DeployStrategy::StopReplaceRestart { process_name } = target.strategy else {
            panic!(
                "expected StopReplaceRestart for ui-server, got {:?}",
                target.strategy
            );
        };
        assert_eq!(process_name, "ui-server");
    }

    #[test]
    fn cli_dest_lives_under_maestro_home() {
        // The CLI deploy target must point at ~maestro/.jam/bin/jam so
        // patch-agent (running as maestro after the systemd-launch
        // switch) can self-update without sudo or the jam-install-bin
        // wrapper. /usr/local/bin/jam and /opt/jam/bin/jam are symlinks
        // pointing at this path, set up once by install-substrate.sh.
        let target = find("cli").unwrap();
        let DeployStrategy::CanonicalBinary { dest_path } = target.strategy else {
            panic!(
                "expected CanonicalBinary for cli, got {:?}",
                target.strategy
            );
        };
        assert!(
            dest_path.starts_with("/home/maestro/"),
            "cli dest_path {dest_path:?} must be under maestro's home so the CLI \
             self-update path is sudo-free; update install-substrate.sh's symlinks \
             to point at the new path if you really mean to move it"
        );
    }

    #[test]
    fn find_returns_none_for_unknown() {
        assert!(find("nonexistent-service").is_none());
        assert!(binary_name("nonexistent-service").is_none());
    }

    #[test]
    fn all_short_names_includes_svc_and_non_svc() {
        let names: Vec<_> = all_short_names().collect();
        assert!(names.contains(&"worktree"));
        assert!(names.contains(&"ui-server"));
    }
}
