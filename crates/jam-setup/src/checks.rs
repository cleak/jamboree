//! The check set itself.

use std::path::Path;
use std::process::Command;

/// Whether a check passed, warned, or failed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckStatus {
    /// All good.
    Pass,
    /// Non-blocking concern — surface but continue.
    Warn,
    /// Blocking — `jam setup` refuses to proceed.
    Fail,
    /// Not yet implemented; informational only.
    Skip,
}

/// How seriously to treat the check.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CheckSeverity {
    /// Required for the orchestrator to function.
    Required,
    /// Strongly recommended; advisory failure.
    Recommended,
}

/// Result of running one [`Check`].
#[derive(Debug, Clone)]
pub struct CheckOutcome {
    /// Stable kebab-case check identifier (e.g. `linux-kernel`, `jam-home-native-fs`).
    pub id: &'static str,
    /// One-line human-readable summary of what was checked.
    pub summary: String,
    /// Pass / Warn / Fail / Skip.
    pub status: CheckStatus,
    /// Severity tier for ranking output and gating `jam setup`.
    pub severity: CheckSeverity,
    /// Multi-line remediation hint when status is Fail/Warn. None on Pass.
    pub remediation: Option<String>,
}

impl CheckOutcome {
    /// Pass shorthand.
    #[must_use]
    pub fn pass(id: &'static str, summary: impl Into<String>) -> Self {
        Self {
            id,
            summary: summary.into(),
            status: CheckStatus::Pass,
            severity: CheckSeverity::Required,
            remediation: None,
        }
    }

    /// Required-fail shorthand with remediation.
    #[must_use]
    pub fn fail(
        id: &'static str,
        summary: impl Into<String>,
        remediation: impl Into<String>,
    ) -> Self {
        Self {
            id,
            summary: summary.into(),
            status: CheckStatus::Fail,
            severity: CheckSeverity::Required,
            remediation: Some(remediation.into()),
        }
    }

    /// Recommended-warn shorthand.
    #[must_use]
    pub fn warn(
        id: &'static str,
        summary: impl Into<String>,
        remediation: impl Into<String>,
    ) -> Self {
        Self {
            id,
            summary: summary.into(),
            status: CheckStatus::Warn,
            severity: CheckSeverity::Recommended,
            remediation: Some(remediation.into()),
        }
    }

    /// Skip (not yet implemented) shorthand.
    #[must_use]
    pub fn skip(id: &'static str, summary: impl Into<String>) -> Self {
        Self {
            id,
            summary: summary.into(),
            status: CheckStatus::Skip,
            severity: CheckSeverity::Recommended,
            remediation: None,
        }
    }
}

/// One environment check.
pub trait Check: Send + Sync {
    /// Run the check and produce an outcome.
    fn run(&self) -> CheckOutcome;
}

/// Run every check and collect outcomes in a stable order.
///
/// Used by `jam setup` (gates install) and `jam doctor` (reports status).
#[must_use]
pub fn run_all_checks() -> Vec<CheckOutcome> {
    let checks: Vec<Box<dyn Check>> = vec![
        // ─── Spec §11.4 base 13 ───────────────────────────────────────
        Box::new(LinuxKernelCheck),
        Box::new(JamHomeNativeFsCheck),
        Box::new(WorktreeRootNativeFsCheck),
        Box::new(TempyrCanonicalWorktreeNativeFsCheck),
        Box::new(InotifyMaxUserWatchesCheck),
        Box::new(SystemdAvailableCheck),
        Box::new(NtpSyncedCheck),
        Box::new(ClockSkewVsNatsCheck),
        Box::new(PassFunctionalCheck),
        Box::new(GpgAgentRunningCheck),
        Box::new(NatsServerReachableCheck),
        Box::new(HarnessesInstalledAtPinnedVersionsCheck),
        Box::new(GithubAppKeyValidCheck),
        // ─── Security-setup §10 multi-user additions (11) ─────────────
        Box::new(ServiceUsersExistCheck),
        Box::new(CallingUserInMaestroGroupCheck),
        Box::new(SudoersConfigPresentCheck),
        Box::new(SudoTransitionWorksCheck),
        Box::new(BootstrapLogPresentCheck),
        Box::new(JamHomeCurrentProcessNativeFsCheck),
        Box::new(SkillsRepoReadableCheck),
        Box::new(CanonicalTempyrWorktreeOwnershipCheck),
        Box::new(MaestroPassStoreHasExpectedKeysCheck),
        Box::new(PickerSpawnSmokeTestCheck),
        Box::new(PickerCannotSudoCheck),
    ];
    checks.iter().map(|c| c.run()).collect()
}

// ─── Implemented checks ────────────────────────────────────────────────

/// Refuse non-Linux outright. WSL is detected and accepted.
pub struct LinuxKernelCheck;
impl Check for LinuxKernelCheck {
    fn run(&self) -> CheckOutcome {
        // Compiled-only: this crate is workspace-restricted to Linux via
        // principle-linux-only-deployment. Compile target catches macOS/Windows.
        #[cfg(target_os = "linux")]
        {
            let release = std::fs::read_to_string("/proc/sys/kernel/osrelease")
                .map_or_else(|_| "unknown".into(), |s| s.trim().to_string());
            CheckOutcome::pass("linux-kernel", format!("Linux kernel detected ({release})"))
        }
        #[cfg(not(target_os = "linux"))]
        {
            CheckOutcome::fail(
                "linux-kernel",
                "Non-Linux platform",
                "Orchestrator does not support macOS or native Windows. \
                 Run inside WSL2 with a Linux distro.",
            )
        }
    }
}

/// Verify the JAM_HOME path canonicalizes to a Linux native filesystem.
pub struct JamHomeNativeFsCheck;
impl Check for JamHomeNativeFsCheck {
    fn run(&self) -> CheckOutcome {
        let jam_home = std::env::var("JAM_HOME").unwrap_or_else(|_| {
            // Default per spec §7.1: /home/maestro/.jam when run as maestro,
            // ~/.jam otherwise. We don't know which user is running yet, so
            // accept either and verify whichever is set.
            "/home/maestro/.jam".into()
        });
        let path = Path::new(&jam_home);
        check_path_is_native_fs("jam-home-native-fs", "JAM_HOME", path)
    }
}

/// Verify the Picker worktree-root canonicalizes to native FS.
pub struct WorktreeRootNativeFsCheck;
impl Check for WorktreeRootNativeFsCheck {
    fn run(&self) -> CheckOutcome {
        // Per dec-blueberry-jam-path: under the multi-user model, Picker
        // worktrees live at /home/picker/workers/.
        let path = Path::new("/home/picker/workers");
        check_path_is_native_fs("worktree-root-native-fs", "worktree root", path)
    }
}

/// Verify the canonical Tempyr worktree is on native FS.
pub struct TempyrCanonicalWorktreeNativeFsCheck;
impl Check for TempyrCanonicalWorktreeNativeFsCheck {
    fn run(&self) -> CheckOutcome {
        let path = Path::new("/home/caleb/blueberry-jam");
        check_path_is_native_fs(
            "tempyr-canonical-worktree-native-fs",
            "canonical Tempyr worktree",
            path,
        )
    }
}

/// `fs.inotify.max_user_watches >= 524288` per §11.4 check #5.
pub struct InotifyMaxUserWatchesCheck;

const MIN_INOTIFY_WATCHES: u64 = 524_288;

impl Check for InotifyMaxUserWatchesCheck {
    fn run(&self) -> CheckOutcome {
        let path = Path::new("/proc/sys/fs/inotify/max_user_watches");
        let raw = match std::fs::read_to_string(path) {
            Ok(s) => s,
            Err(e) => {
                return CheckOutcome::fail(
                    "inotify-max-user-watches",
                    format!("cannot read {}: {e}", path.display()),
                    "verify /proc is mounted; ensure procfs is available",
                );
            }
        };
        let current: u64 = raw.trim().parse().unwrap_or(0);
        if current >= MIN_INOTIFY_WATCHES {
            CheckOutcome::pass(
                "inotify-max-user-watches",
                format!("fs.inotify.max_user_watches = {current} (>= {MIN_INOTIFY_WATCHES})"),
            )
        } else {
            CheckOutcome::fail(
                "inotify-max-user-watches",
                format!("fs.inotify.max_user_watches = {current} (need >= {MIN_INOTIFY_WATCHES})"),
                "echo 'fs.inotify.max_user_watches=524288' \
                 | sudo tee -a /etc/sysctl.d/99-jam.conf\n\
                 sudo sysctl --system",
            )
        }
    }
}

/// Best-effort check that systemd is available (or WSL has systemd=true).
pub struct SystemdAvailableCheck;
impl Check for SystemdAvailableCheck {
    fn run(&self) -> CheckOutcome {
        let pid1 = std::fs::read_link("/proc/1/exe")
            .map(|p| p.display().to_string())
            .unwrap_or_default();
        if pid1.contains("systemd") {
            CheckOutcome::pass("systemd-available", "systemd is PID 1")
        } else if Path::new("/proc/version")
            .canonicalize()
            .map(|p| p.exists())
            .unwrap_or(false)
            && std::fs::read_to_string("/proc/version")
                .map(|s| s.to_lowercase().contains("microsoft") || s.to_lowercase().contains("wsl"))
                .unwrap_or(false)
        {
            CheckOutcome::warn(
                "systemd-available",
                "WSL detected; verify systemd=true in /etc/wsl.conf",
                "Add to /etc/wsl.conf:\n    [boot]\n    systemd=true\n\
                 then run: wsl --shutdown (in PowerShell) and reopen WSL",
            )
        } else {
            CheckOutcome::warn(
                "systemd-available",
                "systemd not detected as PID 1",
                "Optional but recommended for cleaner service lifecycle.",
            )
        }
    }
}

/// `timedatectl show -p NTPSynchronized` returns yes.
pub struct NtpSyncedCheck;
impl Check for NtpSyncedCheck {
    fn run(&self) -> CheckOutcome {
        let output = match Command::new("timedatectl")
            .args(["show", "-p", "NTPSynchronized", "--value"])
            .output()
        {
            Ok(out) => out,
            Err(e) => {
                return CheckOutcome::warn(
                    "ntp-synced",
                    format!("cannot invoke timedatectl: {e}"),
                    "Install systemd-timesyncd or ntp; required for cross-machine traces.",
                );
            }
        };
        let stdout = String::from_utf8_lossy(&output.stdout);
        let value = stdout.trim();
        if value.eq_ignore_ascii_case("yes") {
            CheckOutcome::pass("ntp-synced", "NTP synchronized")
        } else {
            CheckOutcome::fail(
                "ntp-synced",
                format!("NTPSynchronized = {value}"),
                "sudo systemctl enable --now systemd-timesyncd",
            )
        }
    }
}

// ─── Stubbed checks (Skip) ─────────────────────────────────────────────
//
// These checks need infrastructure that doesn't exist yet (NATS reachable,
// pass functional, harnesses pinned, GitHub App). Each returns Skip with a
// pointer to the eventual implementation. Kept in run_all_checks so the
// overall progression is visible at every `jam doctor`.

/// Clock skew vs NATS server < 1s — needs NATS reachable first.
pub struct ClockSkewVsNatsCheck;
impl Check for ClockSkewVsNatsCheck {
    fn run(&self) -> CheckOutcome {
        CheckOutcome::skip(
            "clock-skew-vs-nats",
            "needs running NATS; deferred until process-compose lands",
        )
    }
}

/// `pass` works (test with synthetic key).
pub struct PassFunctionalCheck;
impl Check for PassFunctionalCheck {
    fn run(&self) -> CheckOutcome {
        match Command::new("pass").arg("ls").output() {
            Ok(out) if out.status.success() => {
                CheckOutcome::pass("pass-functional", "pass invocation succeeded")
            }
            Ok(_) => CheckOutcome::warn(
                "pass-functional",
                "pass exited nonzero — store may be empty",
                "If you haven't run init-maestro-keyring.sh + seed-maestro-secrets.sh yet, do so.",
            ),
            Err(e) => CheckOutcome::fail(
                "pass-functional",
                format!("pass not on PATH: {e}"),
                "sudo apt install pass",
            ),
        }
    }
}

/// gpg-agent running with working pinentry.
pub struct GpgAgentRunningCheck;
impl Check for GpgAgentRunningCheck {
    fn run(&self) -> CheckOutcome {
        match Command::new("gpgconf").arg("--check-programs").output() {
            Ok(out) if out.status.success() => {
                CheckOutcome::pass("gpg-agent-running", "gpgconf reports programs OK")
            }
            Ok(_) => CheckOutcome::warn(
                "gpg-agent-running",
                "gpgconf reports issues",
                "Run: gpgconf --check-programs to see details.",
            ),
            Err(e) => CheckOutcome::fail(
                "gpg-agent-running",
                format!("gpgconf not on PATH: {e}"),
                "sudo apt install gnupg pinentry-curses",
            ),
        }
    }
}

/// NATS reachable on configured URL — wired when jam-nats client is invoked from CLI.
pub struct NatsServerReachableCheck;
impl Check for NatsServerReachableCheck {
    fn run(&self) -> CheckOutcome {
        CheckOutcome::skip(
            "nats-server-reachable",
            "deferred until jam-cli wires async runtime + NATS connect",
        )
    }
}

/// Harnesses installed at pinned versions per harness lockfile.
pub struct HarnessesInstalledAtPinnedVersionsCheck;
impl Check for HarnessesInstalledAtPinnedVersionsCheck {
    fn run(&self) -> CheckOutcome {
        CheckOutcome::skip(
            "harnesses-installed",
            "deferred until per-project harness lockfile lands",
        )
    }
}

/// GitHub App key valid (test octocrab token exchange).
pub struct GithubAppKeyValidCheck;
impl Check for GithubAppKeyValidCheck {
    fn run(&self) -> CheckOutcome {
        CheckOutcome::skip("github-app-key-valid", "deferred until jam-svc-repo lands")
    }
}

// ─── Multi-user additions (security-setup §10) ─────────────────────────

/// Service users `maestro` (UID 2000) and `picker` (UID 2001) exist.
pub struct ServiceUsersExistCheck;
impl Check for ServiceUsersExistCheck {
    fn run(&self) -> CheckOutcome {
        for (name, expected_uid) in [("maestro", 2000), ("picker", 2001)] {
            match Command::new("id").arg("-u").arg(name).output() {
                Ok(out) if out.status.success() => {
                    let uid: u32 = String::from_utf8_lossy(&out.stdout)
                        .trim()
                        .parse()
                        .unwrap_or(0);
                    if uid != expected_uid {
                        return CheckOutcome::warn(
                            "service-users-exist",
                            format!("user {name} exists with UID {uid} (expected {expected_uid})"),
                            "Either accept the existing UID or recreate the user.",
                        );
                    }
                }
                _ => {
                    return CheckOutcome::fail(
                        "service-users-exist",
                        format!("user {name} not found"),
                        "sudo ./scripts/bootstrap-users.sh",
                    );
                }
            }
        }
        CheckOutcome::pass(
            "service-users-exist",
            "maestro (UID 2000) and picker (UID 2001) present",
        )
    }
}

/// Calling user is in the `maestro` group.
pub struct CallingUserInMaestroGroupCheck;
impl Check for CallingUserInMaestroGroupCheck {
    fn run(&self) -> CheckOutcome {
        match Command::new("id").arg("-nG").output() {
            Ok(out) if out.status.success() => {
                let groups = String::from_utf8_lossy(&out.stdout);
                if groups.split_whitespace().any(|g| g == "maestro") {
                    CheckOutcome::pass(
                        "calling-user-in-maestro-group",
                        "current user is in the maestro group",
                    )
                } else {
                    CheckOutcome::fail(
                        "calling-user-in-maestro-group",
                        "current user is not in the maestro group",
                        "sudo usermod -aG maestro $USER\n\
                         then log out and back in (or `newgrp maestro`)",
                    )
                }
            }
            _ => CheckOutcome::warn(
                "calling-user-in-maestro-group",
                "could not read group memberships",
                "verify `id -nG` works in your shell",
            ),
        }
    }
}

/// `/etc/sudoers.d/jam-users` present.
///
/// Validation via `visudo -c` requires root and is run by `bootstrap-users.sh`
/// at install time; this check just verifies the file exists and is readable
/// (the bootstrap script's `visudo -c` has already gated installation).
pub struct SudoersConfigPresentCheck;
impl Check for SudoersConfigPresentCheck {
    fn run(&self) -> CheckOutcome {
        let path = Path::new("/etc/sudoers.d/jam-users");
        if path.exists() {
            CheckOutcome::pass(
                "sudoers-config-present",
                format!("{} exists", path.display()),
            )
        } else {
            CheckOutcome::fail(
                "sudoers-config-present",
                format!("{} missing", path.display()),
                "sudo ./scripts/bootstrap-users.sh",
            )
        }
    }
}

/// `sudo -n -u maestro id` succeeds without password.
pub struct SudoTransitionWorksCheck;
impl Check for SudoTransitionWorksCheck {
    fn run(&self) -> CheckOutcome {
        match Command::new("sudo")
            .args(["-n", "-u", "maestro", "id", "-u"])
            .output()
        {
            Ok(out) if out.status.success() => CheckOutcome::pass(
                "sudo-transition-works",
                "sudo -u maestro (NOPASSWD) succeeds",
            ),
            _ => CheckOutcome::warn(
                "sudo-transition-works",
                "sudo to maestro without password failed",
                "Likely group-membership refresh needed: log out and back in.\n\
                 If still failing: sudo cat /etc/sudoers.d/jam-users\n\
                 Re-run: sudo ./scripts/bootstrap-users.sh",
            ),
        }
    }
}

/// `/etc/jam/bootstrap.log` present.
pub struct BootstrapLogPresentCheck;
impl Check for BootstrapLogPresentCheck {
    fn run(&self) -> CheckOutcome {
        let path = Path::new("/etc/jam/bootstrap.log");
        if path.exists() {
            CheckOutcome::pass("bootstrap-log-present", "/etc/jam/bootstrap.log exists")
        } else {
            CheckOutcome::fail(
                "bootstrap-log-present",
                "/etc/jam/bootstrap.log missing",
                "sudo ./scripts/bootstrap-users.sh",
            )
        }
    }
}

/// JAM_HOME for current process is on native FS.
pub struct JamHomeCurrentProcessNativeFsCheck;
impl Check for JamHomeCurrentProcessNativeFsCheck {
    fn run(&self) -> CheckOutcome {
        let home = std::env::var("HOME").unwrap_or_default();
        let path = Path::new(&home);
        check_path_is_native_fs("jam-home-current-process-native-fs", "$HOME", path)
    }
}

/// Skills repo path readable by the running user.
pub struct SkillsRepoReadableCheck;
impl Check for SkillsRepoReadableCheck {
    fn run(&self) -> CheckOutcome {
        let path = Path::new("/home/caleb/jamboree/skills");
        if !path.exists() {
            return CheckOutcome::fail(
                "skills-repo-readable",
                format!("{} missing", path.display()),
                "Verify monorepo is at /home/caleb/jamboree/.\n\
                 Run: sudo ./scripts/bootstrap-users.sh    # sets perms",
            );
        }
        match std::fs::read_dir(path) {
            Ok(_) => CheckOutcome::pass(
                "skills-repo-readable",
                format!("{} readable", path.display()),
            ),
            Err(e) => CheckOutcome::fail(
                "skills-repo-readable",
                format!("read failed on {}: {e}", path.display()),
                "Check group membership (caleb in maestro group) and \
                 directory mode (2770 caleb:maestro).",
            ),
        }
    }
}

/// Canonical Tempyr worktree group ownership + setgid.
pub struct CanonicalTempyrWorktreeOwnershipCheck;
impl Check for CanonicalTempyrWorktreeOwnershipCheck {
    fn run(&self) -> CheckOutcome {
        let path = Path::new("/home/caleb/blueberry-jam");
        if !path.exists() {
            return CheckOutcome::warn(
                "canonical-tempyr-worktree-ownership",
                format!("{} missing — not yet created", path.display()),
                "Per dec-blueberry-jam-path:\n\
                 sudo -u caleb -i git -C /home/caleb/blueberry \\\n  \
                 worktree add /home/caleb/blueberry-jam tempyr-live\n\
                 sudo chown -R caleb:maestro /home/caleb/blueberry-jam\n\
                 sudo find /home/caleb/blueberry-jam -type d -exec chmod 2770 {} \\;",
            );
        }
        // For Phase 0 we just confirm existence; full ownership/setgid check
        // requires nix syscalls and is deferred.
        CheckOutcome::pass(
            "canonical-tempyr-worktree-ownership",
            format!("{} exists (full perm audit deferred)", path.display()),
        )
    }
}

/// maestro's pass store has expected keys.
pub struct MaestroPassStoreHasExpectedKeysCheck;
impl Check for MaestroPassStoreHasExpectedKeysCheck {
    fn run(&self) -> CheckOutcome {
        CheckOutcome::skip(
            "maestro-pass-store-has-expected-keys",
            "deferred until jam-secrets is wired into the CLI",
        )
    }
}

/// Picker spawn smoke test — verify sudo -u picker can write to /home/picker/workers.
pub struct PickerSpawnSmokeTestCheck;
impl Check for PickerSpawnSmokeTestCheck {
    fn run(&self) -> CheckOutcome {
        match Command::new("sudo")
            .args(["-n", "-u", "picker", "test", "-w", "/home/picker/workers"])
            .output()
        {
            Ok(out) if out.status.success() => CheckOutcome::pass(
                "picker-spawn-smoke-test",
                "picker can write to /home/picker/workers",
            ),
            _ => CheckOutcome::warn(
                "picker-spawn-smoke-test",
                "sudo -u picker test -w /home/picker/workers failed",
                "Run: sudo ./scripts/bootstrap-users.sh\n\
                 Verify picker home perms: ls -ld /home/picker/workers",
            ),
        }
    }
}

/// Verify picker user CANNOT sudo (least privilege).
pub struct PickerCannotSudoCheck;
impl Check for PickerCannotSudoCheck {
    fn run(&self) -> CheckOutcome {
        // We expect this to FAIL — that's the desired state.
        // We invoke `sudo -n` from `caleb`, sudo'ing to picker, then trying to sudo.
        match Command::new("sudo")
            .args(["-n", "-u", "picker", "sudo", "-n", "-u", "root", "true"])
            .output()
        {
            Ok(out) if out.status.success() => CheckOutcome::fail(
                "picker-cannot-sudo",
                "picker can sudo — security regression",
                "Inspect /etc/sudoers.d/jam-users; rerun bootstrap-users.sh.",
            ),
            _ => CheckOutcome::pass(
                "picker-cannot-sudo",
                "picker cannot sudo (least privilege confirmed)",
            ),
        }
    }
}

// ─── Helpers ──────────────────────────────────────────────────────────

fn check_path_is_native_fs(id: &'static str, label: &str, path: &Path) -> CheckOutcome {
    if !path.exists() {
        return CheckOutcome::warn(
            id,
            format!("{label}: {} does not exist yet", path.display()),
            "Will be created when its owning step runs.",
        );
    }
    let canonical = match path.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            return CheckOutcome::fail(
                id,
                format!("cannot canonicalize {}: {e}", path.display()),
                "Verify the path is accessible to the calling user.",
            );
        }
    };
    if is_windows_mount(&canonical) {
        CheckOutcome::fail(
            id,
            format!("{label} is on a Windows mount: {}", canonical.display()),
            "Per spec §2.14 / principle-native-fs-only: orchestrator data \
             must live on Linux native FS.\n\
             Move to /home/<user>/...",
        )
    } else {
        CheckOutcome::pass(id, format!("{label} on native FS: {}", canonical.display()))
    }
}

/// Spec §6.6 canonical implementation: refuse `/mnt/<lower>/`, `/cygdrive/`.
///
/// Catches `/mnt/c`, `/mnt/c/Users/...`, `/cygdrive/c/...`, etc. Multi-letter
/// segments after `/mnt/` (e.g. `/mnt/data`) are NOT flagged so legitimate
/// Linux external-disk mounts pass.
fn is_windows_mount(path: &Path) -> bool {
    let s = path.to_string_lossy();
    if s.starts_with("/cygdrive/") {
        return true;
    }
    let Some(rest) = s.strip_prefix("/mnt/") else {
        return false;
    };
    let mut chars = rest.chars();
    let Some(c) = chars.next() else {
        return false;
    };
    if !c.is_ascii_lowercase() {
        return false;
    }
    // Single-letter segment: either nothing follows, or the next char is `/`.
    matches!(chars.next(), None | Some('/'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn is_windows_mount_recognizes_drvfs() {
        assert!(is_windows_mount(&PathBuf::from("/mnt/c/Users/caleb")));
        assert!(is_windows_mount(&PathBuf::from("/mnt/d")));
        assert!(is_windows_mount(&PathBuf::from("/cygdrive/c/temp")));
    }

    #[test]
    fn is_windows_mount_accepts_native() {
        assert!(!is_windows_mount(&PathBuf::from("/home/caleb")));
        assert!(!is_windows_mount(&PathBuf::from("/opt/jam/bin")));
        // /mnt/data is a legitimate external disk mount on some hosts; the
        // single-letter check allows multi-letter segments to pass.
        assert!(!is_windows_mount(&PathBuf::from("/mnt/data")));
        assert!(!is_windows_mount(&PathBuf::from("/mnt/D"))); // uppercase: not drvfs
    }

    #[test]
    fn run_all_checks_returns_24_outcomes() {
        let outcomes = run_all_checks();
        assert_eq!(
            outcomes.len(),
            24,
            "spec §11.4 (13) + security-setup §10 (11)"
        );
    }

    #[test]
    fn each_outcome_has_stable_id() {
        let outcomes = run_all_checks();
        let mut ids: Vec<&str> = outcomes.iter().map(|o| o.id).collect();
        ids.sort_unstable();
        let len = ids.len();
        ids.dedup();
        assert_eq!(ids.len(), len, "duplicate check IDs");
    }
}
