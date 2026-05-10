//! The check set itself.

use std::collections::{BTreeMap, BTreeSet};
use std::io::{BufRead as _, Read as _};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

#[cfg(unix)]
use std::os::unix::fs::{MetadataExt, PermissionsExt};

use octocrab::models::{AppId, InstallationId};
use octocrab::Octocrab;
use secrecy::{ExposeSecret, SecretString};
use serde::Deserialize;
use sha2::{Digest, Sha256};

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
        Box::new(TraceGapDetectorCheck),
        // ─── Phase 9 learned hardening checks (2) ───────────────────────
        Box::new(RootSudoNoninteractiveCheck),
        Box::new(SubstrateBinariesInstalledCheck),
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
        let jam_home = jam_tools_core::paths::jam_home();
        let path = jam_home.as_path();
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
        check_path_is_native_fs_as_user("worktree-root-native-fs", "worktree root", path, "picker")
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
        let pid1_comm = std::fs::read_to_string("/proc/1/comm")
            .map(|value| value.trim().to_string())
            .unwrap_or_default();
        if pid1.contains("systemd") || pid1_comm == "systemd" {
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

/// Warn when journal traces look broken or too short to explain.
pub struct TraceGapDetectorCheck;
impl Check for TraceGapDetectorCheck {
    fn run(&self) -> CheckOutcome {
        let journal_root = std::env::var_os("JAM_JOURNAL_ROOT").map_or_else(
            || jam_tools_core::paths::jam_home().join("journal"),
            PathBuf::from,
        );
        match trace_gap_report(&journal_root) {
            Ok(report) if report.suspect_traces.is_empty() => CheckOutcome::pass(
                "trace-gap-detector",
                format!(
                    "trace continuity scan found {} traced journal event(s) across {} trace(s)",
                    report.event_count, report.trace_count
                ),
            ),
            Ok(report) => CheckOutcome::warn(
                "trace-gap-detector",
                format!(
                    "{} suspicious single-entry trace(s): {}",
                    report.suspect_traces.len(),
                    report.suspect_traces.join(", ")
                ),
                "Run: jam trace replay <trace-id>\n\
                 A trace that appears once may be legal, but it often means a boundary \
                 dropped Trace-Id / Parent-Trace-Id.",
            ),
            Err(detail) => CheckOutcome::warn(
                "trace-gap-detector",
                detail,
                "Ensure JAM_JOURNAL_ROOT points at a readable native journal directory, \
                 then rerun jam doctor.",
            ),
        }
    }
}

/// Root sudo availability for root-only installers.
pub struct RootSudoNoninteractiveCheck;
impl Check for RootSudoNoninteractiveCheck {
    fn run(&self) -> CheckOutcome {
        match Command::new("sudo").args(["-n", "true"]).output() {
            Ok(out) if out.status.success() => CheckOutcome::pass(
                "root-sudo-noninteractive",
                "root sudo works without prompting in this shell",
            ),
            Ok(out) => CheckOutcome::warn(
                "root-sudo-noninteractive",
                format!(
                    "sudo -n true failed{}",
                    command_failure_suffix(&out.stdout, &out.stderr)
                ),
                "Root-only bootstrap commands such as scripts/install-substrate.sh \
                 must run from an interactive terminal or an existing root shell.\n\
                 Run: sudo ./scripts/install-substrate.sh",
            ),
            Err(e) => CheckOutcome::warn(
                "root-sudo-noninteractive",
                format!("cannot invoke sudo: {e}"),
                "Install sudo or run root-only bootstrap commands from a root shell.",
            ),
        }
    }
}

const SUBSTRATE_INSTALL_DIR: &str = "/opt/jam/bin";
const PROCESS_COMPOSE_CONFIG_PATH: &str = "/home/caleb/jamboree/process-compose.yaml";
const UI_STATIC_DIR: &str = "/home/maestro/.jam/ui/dist";
const REQUIRED_NATS_SERVER_VERSION: &str = "2.11.0";
const REQUIRED_PROCESS_COMPOSE_VERSION: &str = "1.40.1";

#[derive(Debug, Deserialize)]
struct ProcessComposeConfig {
    processes: BTreeMap<String, ProcessComposeProcess>,
}

#[derive(Debug, Deserialize)]
struct ProcessComposeProcess {
    command: Option<String>,
    disabled: Option<bool>,
    readiness_probe: Option<ProcessComposeReadinessProbe>,
}

#[derive(Debug, Deserialize)]
struct ProcessComposeReadinessProbe {
    exec: Option<ProcessComposeReadinessExec>,
}

#[derive(Debug, Deserialize)]
struct ProcessComposeReadinessExec {
    command: Option<String>,
}

/// Pinned substrate binaries installed in the production path.
pub struct SubstrateBinariesInstalledCheck;
impl Check for SubstrateBinariesInstalledCheck {
    fn run(&self) -> CheckOutcome {
        let nats_path = Path::new(SUBSTRATE_INSTALL_DIR).join("nats-server");
        let process_compose_path = Path::new(SUBSTRATE_INSTALL_DIR).join("process-compose");

        let mut problems = Vec::new();
        if let Err(problem) = check_pinned_binary(
            "nats-server",
            &nats_path,
            &["--version"],
            REQUIRED_NATS_SERVER_VERSION,
        ) {
            problems.push(problem);
        }
        if let Err(problem) = check_pinned_binary(
            "process-compose",
            &process_compose_path,
            &["version"],
            REQUIRED_PROCESS_COMPOSE_VERSION,
        ) {
            problems.push(problem);
        }
        problems.extend(enabled_process_binary_problems(
            Path::new(PROCESS_COMPOSE_CONFIG_PATH),
            Path::new(SUBSTRATE_INSTALL_DIR),
        ));
        problems.extend(enabled_ui_static_bundle_problems(
            Path::new(PROCESS_COMPOSE_CONFIG_PATH),
            Path::new(UI_STATIC_DIR),
        ));

        if problems.is_empty() {
            CheckOutcome::pass(
                "substrate-binaries-installed",
                format!(
                    "pinned substrate binaries and enabled process-compose service binaries \
                     are installed in {SUBSTRATE_INSTALL_DIR}; UI static bundle is present",
                ),
            )
        } else {
            CheckOutcome::fail(
                "substrate-binaries-installed",
                problems.join("; "),
                "Run: sudo ./scripts/install-substrate.sh\n\
                 This installs pinned nats-server/process-compose and the \
                 currently enabled first-party service binaries and UI static \
                 bundle from process-compose.yaml.\n\
                 Keep undeployed future services disabled.\n\
                 If sudo prompts in a noninteractive agent shell, run the command \
                 from Caleb's interactive terminal or an existing root shell.\n\
                 Verify:\n\
                   /opt/jam/bin/nats-server --version\n\
                   /opt/jam/bin/process-compose version\n\
                   scripts/smoke-substrate-journal.sh --maestro-runtime\n\
                   scripts/smoke-substrate-journal.sh --existing",
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
        let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into());
        let authority = match nats_authority(&nats_url) {
            Ok(authority) => authority,
            Err(detail) => {
                return CheckOutcome::fail(
                    "clock-skew-vs-nats",
                    format!("invalid NATS_URL {nats_url:?}: {detail}"),
                    "Set NATS_URL to the loopback substrate URL, normally nats://127.0.0.1:4222.",
                );
            }
        };
        if !is_loopback_authority(&authority) {
            return CheckOutcome::fail(
                "clock-skew-vs-nats",
                format!("NATS is not loopback-only: {authority}"),
                "Per dec-single-node-jetstream, run the substrate on loopback NATS so NATS and jam doctor share one host clock.",
            );
        }
        match nats_info_probe(&authority) {
            Ok(()) => CheckOutcome::pass(
                "clock-skew-vs-nats",
                format!("NATS at {authority} is loopback; host clock is shared"),
            ),
            Err(detail) => CheckOutcome::fail(
                "clock-skew-vs-nats",
                format!(
                    "cannot verify NATS clock because NATS is unreachable at {authority}: {detail}"
                ),
                "Start NATS with process-compose, then rerun jam doctor.",
            ),
        }
    }
}

/// `pass` works (test with synthetic key).
pub struct PassFunctionalCheck;
impl Check for PassFunctionalCheck {
    fn run(&self) -> CheckOutcome {
        match Command::new("sudo")
            .args(["-n", "-u", "maestro", "-H", "pass", "ls"])
            .output()
        {
            Ok(out) if out.status.success() => {
                CheckOutcome::pass("pass-functional", "maestro pass invocation succeeded")
            }
            Ok(_) => CheckOutcome::warn(
                "pass-functional",
                "maestro pass exited nonzero — store may be empty or uninitialized",
                "If you haven't run init-maestro-keyring.sh + seed-maestro-secrets.sh yet, do so.",
            ),
            Err(e) => CheckOutcome::fail(
                "pass-functional",
                format!("cannot invoke sudo/pass: {e}"),
                "sudo apt install pass",
            ),
        }
    }
}

/// gpg-agent running with working pinentry.
pub struct GpgAgentRunningCheck;
impl Check for GpgAgentRunningCheck {
    fn run(&self) -> CheckOutcome {
        match Command::new("sudo")
            .args(["-n", "-u", "maestro", "-H", "gpg-connect-agent", "/bye"])
            .output()
        {
            Ok(out) if out.status.success() => {
                CheckOutcome::pass("gpg-agent-running", "maestro gpg-agent responds")
            }
            Ok(out) => CheckOutcome::warn(
                "gpg-agent-running",
                format!(
                    "maestro gpg-agent check failed{}",
                    command_failure_suffix(&out.stdout, &out.stderr)
                ),
                "Run: sudo -u maestro -H gpg-connect-agent /bye\n\
                 If needed, initialize maestro GPG/pass per docs/security-setup.md §5.",
            ),
            Err(e) => CheckOutcome::fail(
                "gpg-agent-running",
                format!("cannot invoke sudo/gpg-connect-agent: {e}"),
                "sudo apt install gnupg pinentry-curses",
            ),
        }
    }
}

/// NATS reachable on configured URL — wired when jam-nats client is invoked from CLI.
pub struct NatsServerReachableCheck;
impl Check for NatsServerReachableCheck {
    fn run(&self) -> CheckOutcome {
        let nats_url = std::env::var("NATS_URL").unwrap_or_else(|_| "nats://127.0.0.1:4222".into());
        let authority = match nats_authority(&nats_url) {
            Ok(authority) => authority,
            Err(detail) => {
                return CheckOutcome::fail(
                    "nats-server-reachable",
                    format!("invalid NATS_URL {nats_url:?}: {detail}"),
                    "Set NATS_URL to a loopback NATS URL such as nats://127.0.0.1:4222.",
                );
            }
        };
        match nats_info_probe(&authority) {
            Ok(()) => CheckOutcome::pass(
                "nats-server-reachable",
                format!("NATS server reachable at {authority}"),
            ),
            Err(detail) => CheckOutcome::fail(
                "nats-server-reachable",
                format!("NATS server not reachable at {authority}: {detail}"),
                "Start the substrate with process-compose or run the pinned server:\n\
                 sudo /opt/jam/bin/process-compose -f /home/caleb/jamboree/process-compose.yaml up nats",
            ),
        }
    }
}

/// Harnesses installed at pinned versions per harness lockfile.
pub struct HarnessesInstalledAtPinnedVersionsCheck;
impl Check for HarnessesInstalledAtPinnedVersionsCheck {
    fn run(&self) -> CheckOutcome {
        let lockfile_path = std::env::var_os("JAM_HARNESS_LOCKFILE")
            .map_or_else(default_harness_lockfile_path, PathBuf::from);
        match check_harness_lockfile(&lockfile_path) {
            Ok(summary) => CheckOutcome::pass("harnesses-installed", summary),
            Err(detail) => CheckOutcome::fail(
                "harnesses-installed",
                detail,
                "Create /home/maestro/.jam/config/projects/blueberry-harnesses.lock \
                 from docs/onboard-blueberry.md, then run the harness validation workflow \
                 before accepting new pins.",
            ),
        }
    }
}

/// GitHub App key valid (test octocrab token exchange).
pub struct GithubAppKeyValidCheck;
impl Check for GithubAppKeyValidCheck {
    fn run(&self) -> CheckOutcome {
        let config = match collect_github_app_doctor_config() {
            Ok(Some(config)) => config,
            Ok(None) => {
                return CheckOutcome::warn(
                    "github-app-key-valid",
                    "GitHub App credentials are not configured",
                    "Seed env vars JAM_GITHUB_APP_ID, JAM_GITHUB_APP_INSTALLATION_ID, \
                     and JAM_GITHUB_APP_PRIVATE_KEY/JAM_GITHUB_APP_PRIVATE_KEY_FILE, \
                     or seed maestro pass keys jam/pickers/github-app-id, \
                     jam/pickers/github-app-installation-id, and \
                     jam/pickers/github-app-key.",
                );
            }
            Err(outcome) => return outcome,
        };

        match github_app_token_exchange_check(&config) {
            Ok(()) => CheckOutcome::pass(
                "github-app-key-valid",
                format!(
                    "GitHub App token exchange succeeded for installation {}",
                    config.installation_id
                ),
            ),
            Err(detail) => CheckOutcome::fail(
                "github-app-key-valid",
                format!("GitHub App token exchange failed: {detail}"),
                "Verify the App ID, installation ID, and private key are current; \
                 verify the App is installed on the Blueberry repo.",
            ),
        }
    }
}

#[derive(Debug, Clone)]
struct GithubAppDoctorConfig {
    app_id: u64,
    installation_id: u64,
    private_key_pem: SecretString,
    api_base_uri: Option<String>,
}

#[derive(Debug, Default)]
struct GithubAppRawConfig {
    app_id: Option<String>,
    installation_id: Option<String>,
    private_key_pem: Option<String>,
    api_base_uri: Option<String>,
}

fn collect_github_app_doctor_config() -> Result<Option<GithubAppDoctorConfig>, CheckOutcome> {
    github_app_doctor_config_from_raw(GithubAppRawConfig {
        app_id: first_secret(
            &["JAM_GITHUB_APP_ID", "GITHUB_APP_ID"],
            &["jam/pickers/github-app-id"],
        ),
        installation_id: first_secret(
            &[
                "JAM_GITHUB_APP_INSTALLATION_ID",
                "GITHUB_APP_INSTALLATION_ID",
            ],
            &["jam/pickers/github-app-installation-id"],
        ),
        private_key_pem: first_secret(
            &["JAM_GITHUB_APP_PRIVATE_KEY", "GITHUB_APP_PRIVATE_KEY"],
            &["jam/pickers/github-app-key"],
        )
        .or_else(|| read_optional_secret_file("JAM_GITHUB_APP_PRIVATE_KEY_FILE"))
        .or_else(|| read_optional_secret_file("GITHUB_APP_PRIVATE_KEY_FILE")),
        api_base_uri: first_env(&["JAM_GITHUB_API_BASE_URI", "GITHUB_API_BASE_URI"]),
    })
}

fn github_app_doctor_config_from_raw(
    raw: GithubAppRawConfig,
) -> Result<Option<GithubAppDoctorConfig>, CheckOutcome> {
    if raw.app_id.is_none() && raw.installation_id.is_none() && raw.private_key_pem.is_none() {
        return Ok(None);
    }
    let mut missing = Vec::new();
    if raw.app_id.is_none() {
        missing.push("GitHub App ID");
    }
    if raw.installation_id.is_none() {
        missing.push("GitHub App installation ID");
    }
    if raw.private_key_pem.is_none() {
        missing.push("GitHub App private key PEM");
    }
    if !missing.is_empty() {
        return Err(CheckOutcome::fail(
            "github-app-key-valid",
            format!(
                "partial GitHub App config is missing {}",
                missing.join(", ")
            ),
            "Set all of JAM_GITHUB_APP_ID, JAM_GITHUB_APP_INSTALLATION_ID, \
             and JAM_GITHUB_APP_PRIVATE_KEY/JAM_GITHUB_APP_PRIVATE_KEY_FILE, \
             or seed all three maestro pass keys under jam/pickers/.",
        ));
    }

    let app_id = parse_github_app_u64("GitHub App ID", raw.app_id.as_deref().unwrap_or_default())?;
    let installation_id = parse_github_app_u64(
        "GitHub App installation ID",
        raw.installation_id.as_deref().unwrap_or_default(),
    )?;
    let private_key_pem = raw.private_key_pem.unwrap_or_default().replace("\\n", "\n");
    Ok(Some(GithubAppDoctorConfig {
        app_id,
        installation_id,
        private_key_pem: SecretString::from(private_key_pem),
        api_base_uri: raw.api_base_uri,
    }))
}

fn parse_github_app_u64(field: &'static str, raw: &str) -> Result<u64, CheckOutcome> {
    raw.trim().parse::<u64>().map_err(|err| {
        CheckOutcome::fail(
            "github-app-key-valid",
            format!("{field} is not an unsigned integer: {err}"),
            "Use the numeric GitHub App and installation IDs from the App settings.",
        )
    })
}

fn github_app_token_exchange_check(config: &GithubAppDoctorConfig) -> Result<(), String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|err| format!("failed to build async runtime: {err}"))?;
    runtime.block_on(async move {
        let key = jsonwebtoken::EncodingKey::from_rsa_pem(
            config.private_key_pem.expose_secret().as_bytes(),
        )
        .map_err(|err| format!("private key PEM parse failed: {err}"))?;
        let mut builder = Octocrab::builder().app(AppId(config.app_id), key);
        if let Some(base_uri) = &config.api_base_uri {
            builder = builder
                .base_uri(base_uri)
                .map_err(|err| format!("invalid GitHub API base URI {base_uri:?}: {err}"))?;
        }
        let crab = builder
            .build()
            .map_err(|err| format!("GitHub App client build failed: {err}"))?;
        crab.installation_and_token(InstallationId(config.installation_id))
            .await
            .map(|_| ())
            .map_err(|err| err.to_string())
    })
}

fn first_secret(env_names: &[&str], pass_keys: &[&str]) -> Option<String> {
    first_env(env_names).or_else(|| pass_keys.iter().find_map(|key| read_pass_secret(key)))
}

fn first_env(names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        std::env::var(name)
            .ok()
            .map(|value| value.trim_end().to_owned())
            .filter(|value| !value.is_empty())
    })
}

fn read_optional_secret_file(env_name: &str) -> Option<String> {
    let path = std::env::var_os(env_name)?;
    std::fs::read_to_string(path)
        .ok()
        .map(|value| value.trim_end().to_owned())
        .filter(|value| !value.is_empty())
}

fn read_pass_secret(key: &str) -> Option<String> {
    command_stdout("sudo", &["-n", "-u", "maestro", "-H", "pass", "show", key])
        .or_else(|| command_stdout("pass", &["show", key]))
        .map(|value| value.trim_end().to_owned())
        .filter(|value| !value.is_empty())
}

fn command_stdout(program: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(program).args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    String::from_utf8(output.stdout).ok()
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
        let jam_home = jam_tools_core::paths::jam_home();
        check_path_is_native_fs(
            "jam-home-current-process-native-fs",
            "resolved JAM_HOME",
            jam_home.as_path(),
        )
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
        match audit_canonical_tempyr_worktree(path) {
            Ok(summary) => CheckOutcome::pass("canonical-tempyr-worktree-ownership", summary),
            Err(detail) => CheckOutcome::fail(
                "canonical-tempyr-worktree-ownership",
                detail,
                "Repair the shared worktree permissions:\n\
                 chgrp -R maestro /home/caleb/blueberry-jam\n\
                 chmod -R g+rwX,o-rwx /home/caleb/blueberry-jam\n\
                 find /home/caleb/blueberry-jam -type d -exec chmod g+s {} \\;",
            ),
        }
    }
}

/// maestro's pass store has expected keys.
pub struct MaestroPassStoreHasExpectedKeysCheck;
impl Check for MaestroPassStoreHasExpectedKeysCheck {
    fn run(&self) -> CheckOutcome {
        let missing = EXPECTED_MAESTRO_PASS_KEYS
            .iter()
            .copied()
            .filter(|key| !maestro_pass_key_present(key))
            .collect::<Vec<_>>();
        if missing.is_empty() {
            CheckOutcome::pass(
                "maestro-pass-store-has-expected-keys",
                "recommended maestro pass keys are present",
            )
        } else {
            CheckOutcome::warn(
                "maestro-pass-store-has-expected-keys",
                format!(
                    "missing recommended maestro pass keys: {}",
                    missing.join(", ")
                ),
                "Run: ./scripts/seed-maestro-secrets.sh\n\
                 Or insert the missing keys manually with sudo -u maestro -i pass insert ...",
            )
        }
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

const EXPECTED_MAESTRO_PASS_KEYS: &[&str] = &[
    "jam/pickers/github-app-id",
    "jam/pickers/github-app-installation-id",
    "jam/pickers/github-app-key",
    "jam/notify/ntfy-token",
    "jam/search/brave",
];

#[derive(Debug, Deserialize)]
struct DoctorHarnessLockfile {
    #[serde(default)]
    harnesses: BTreeMap<String, DoctorHarnessPin>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "kebab-case")]
struct DoctorHarnessPin {
    version: String,
    checksum_sha256: String,
}

fn maestro_pass_key_present(key: &str) -> bool {
    command_stdout("sudo", &["-n", "-u", "maestro", "-H", "pass", "show", key]).is_some()
}

#[cfg(unix)]
fn audit_canonical_tempyr_worktree(path: &Path) -> Result<String, String> {
    let caleb_uid = numeric_unix_id("passwd", "caleb", 2)?;
    let maestro_gid = numeric_unix_id("group", "maestro", 2)?;
    let metadata =
        std::fs::symlink_metadata(path).map_err(|err| format!("stat {}: {err}", path.display()))?;
    if metadata.uid() != caleb_uid {
        return Err(format!(
            "{} owner uid is {}, expected caleb ({caleb_uid})",
            path.display(),
            metadata.uid()
        ));
    }
    audit_maestro_group_tree(path, maestro_gid)?;
    Ok(format!(
        "{} is caleb-owned with maestro group and setgid directories",
        path.display()
    ))
}

#[cfg(not(unix))]
fn audit_canonical_tempyr_worktree(path: &Path) -> Result<String, String> {
    Ok(format!("{} exists", path.display()))
}

#[cfg(unix)]
fn numeric_unix_id(database: &str, name: &str, field_index: usize) -> Result<u32, String> {
    let output = Command::new("getent")
        .args([database, name])
        .output()
        .map_err(|err| format!("run getent {database} {name}: {err}"))?;
    if !output.status.success() {
        return Err(format!("getent {database} {name} failed"));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    stdout
        .trim()
        .split(':')
        .nth(field_index)
        .ok_or_else(|| format!("getent {database} {name} returned an unexpected shape"))?
        .parse::<u32>()
        .map_err(|err| format!("parse {name} id from getent output: {err}"))
}

#[cfg(unix)]
fn audit_maestro_group_tree(path: &Path, maestro_gid: u32) -> Result<(), String> {
    let metadata =
        std::fs::symlink_metadata(path).map_err(|err| format!("stat {}: {err}", path.display()))?;
    if metadata.file_type().is_symlink() {
        return Ok(());
    }
    if metadata.gid() != maestro_gid {
        return Err(format!(
            "{} group gid is {}, expected maestro ({maestro_gid})",
            path.display(),
            metadata.gid()
        ));
    }
    if metadata.is_dir() {
        if metadata.permissions().mode() & 0o2000 == 0 {
            return Err(format!("{} is missing the setgid bit", path.display()));
        }
        for entry in
            std::fs::read_dir(path).map_err(|err| format!("read {}: {err}", path.display()))?
        {
            let entry = entry.map_err(|err| format!("read {}: {err}", path.display()))?;
            audit_maestro_group_tree(&entry.path(), maestro_gid)?;
        }
    }
    Ok(())
}

fn default_harness_lockfile_path() -> PathBuf {
    PathBuf::from("/home/maestro/.jam")
        .join("config")
        .join("projects")
        .join("blueberry-harnesses.lock")
}

fn check_harness_lockfile(lockfile_path: &Path) -> Result<String, String> {
    let raw = read_runtime_text_file(lockfile_path)?;
    let lockfile = toml::from_str::<DoctorHarnessLockfile>(&raw)
        .map_err(|err| format!("failed to parse {}: {err}", lockfile_path.display()))?;
    if lockfile.harnesses.is_empty() {
        return Err(format!(
            "{} contains no [harnesses.*] pins",
            lockfile_path.display()
        ));
    }

    let mut checked = Vec::new();
    for (harness, pin) in lockfile.harnesses {
        if pin.version == "deferred" && pin.checksum_sha256 == "deferred" {
            continue;
        }
        let bin = harness_bin(&harness)?;
        let resolved = resolve_harness_binary(&bin)?;
        let installed_version = harness_version(&resolved)?;
        if installed_version != pin.version {
            return Err(format!(
                "{harness} version is {installed_version}, lockfile expects {}",
                pin.version
            ));
        }
        let checksum = sha256_file(&resolved)
            .map_err(|err| format!("failed to checksum {}: {err}", resolved.display()))?;
        if checksum != pin.checksum_sha256 {
            return Err(format!(
                "{harness} checksum is {checksum}, lockfile expects {}",
                pin.checksum_sha256
            ));
        }
        checked.push(format!("{harness} {installed_version}"));
    }

    if checked.is_empty() {
        Err(format!(
            "{} has only deferred harness pins; at least codex-cli must be pinned",
            lockfile_path.display()
        ))
    } else {
        Ok(format!(
            "harness pins match installed binaries: {}",
            checked.join(", ")
        ))
    }
}

fn harness_bin(harness: &str) -> Result<PathBuf, String> {
    match harness {
        "codex-cli" => runtime_harness_bin("JAM_CODEX_BIN", "codex"),
        "claude-code" => runtime_harness_bin("JAM_CLAUDE_BIN", "claude"),
        "opencode-deepseek" => runtime_harness_bin("JAM_OPENCODE_BIN", "opencode"),
        other => Err(format!(
            "unsupported pinned harness {other}; jam doctor knows codex-cli, claude-code, and opencode-deepseek"
        )),
    }
}

fn read_runtime_text_file(path: &Path) -> Result<String, String> {
    match std::fs::read_to_string(path) {
        Ok(raw) => Ok(raw),
        Err(err) if err.kind() == std::io::ErrorKind::PermissionDenied => {
            let path_arg = path.display().to_string();
            command_stdout("sudo", &["-n", "-u", "maestro", "-H", "cat", &path_arg])
                .ok_or_else(|| format!("failed to read {} as maestro via sudo", path.display()))
        }
        Err(err) => Err(format!("failed to read {}: {err}", path.display())),
    }
}

fn runtime_harness_bin(env_key: &str, default_bin: &str) -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os(env_key).filter(|value| !value.is_empty()) {
        return Ok(PathBuf::from(path));
    }
    let command = format!("command -v {default_bin}");
    command_stdout(
        "sudo",
        &["-n", "-u", "maestro", "-H", "bash", "-lc", &command],
    )
    .map(|path| PathBuf::from(path.trim()))
    .ok_or_else(|| format!("could not find {default_bin} on maestro PATH"))
}

fn resolve_harness_binary(path: &Path) -> Result<PathBuf, String> {
    if path.is_absolute() || path.components().count() > 1 {
        return path
            .canonicalize()
            .map_err(|err| format!("failed to canonicalize {}: {err}", path.display()));
    }
    let path_var = std::env::var_os("PATH").unwrap_or_default();
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(path);
        if candidate.is_file() {
            return candidate
                .canonicalize()
                .map_err(|err| format!("failed to canonicalize {}: {err}", candidate.display()));
        }
    }
    Err(format!("could not find {} on PATH", path.display()))
}

fn harness_version(harness_bin: &Path) -> Result<String, String> {
    let output = Command::new(harness_bin)
        .arg("--version")
        .output()
        .map_err(|err| format!("failed to run {} --version: {err}", harness_bin.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{} --version failed{}",
            harness_bin.display(),
            command_failure_suffix(&output.stdout, &output.stderr)
        ));
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    parse_harness_version(&stdout).ok_or_else(|| {
        format!(
            "{} --version output was not understood: {}",
            harness_bin.display(),
            stdout.trim()
        )
    })
}

fn parse_harness_version(output: &str) -> Option<String> {
    output
        .split_whitespace()
        .find(|part| part.chars().next().is_some_and(|ch| ch.is_ascii_digit()))
        .map(ToOwned::to_owned)
}

fn sha256_file(path: &Path) -> std::io::Result<String> {
    let bytes = std::fs::read(path)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn nats_authority(nats_url: &str) -> Result<String, String> {
    let Some(rest) = nats_url.strip_prefix("nats://") else {
        return Err("only nats:// URLs are supported by jam doctor".into());
    };
    let authority = rest.split('/').next().unwrap_or_default();
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    if authority.is_empty() {
        return Err("missing host:port authority".into());
    }
    if !authority.contains(':') {
        return Err("missing port".into());
    }
    Ok(authority.to_owned())
}

fn is_loopback_authority(authority: &str) -> bool {
    let host = authority
        .rsplit_once(':')
        .map_or(authority, |(host, _)| host)
        .trim_matches(['[', ']']);
    matches!(host, "127.0.0.1" | "::1" | "localhost")
}

fn nats_info_probe(authority: &str) -> Result<(), String> {
    let mut addrs = authority
        .to_socket_addrs()
        .map_err(|err| format!("address resolution failed: {err}"))?;
    let addr = addrs
        .next()
        .ok_or_else(|| "address resolution returned no addresses".to_string())?;
    let timeout = Duration::from_secs(1);
    let mut stream = TcpStream::connect_timeout(&addr, timeout)
        .map_err(|err| format!("connect failed: {err}"))?;
    stream
        .set_read_timeout(Some(timeout))
        .map_err(|err| format!("set read timeout failed: {err}"))?;
    let mut buf = [0_u8; 512];
    let len = stream
        .read(&mut buf)
        .map_err(|err| format!("read INFO failed: {err}"))?;
    if buf[..len].starts_with(b"INFO ") {
        Ok(())
    } else {
        Err("server did not send a NATS INFO line".into())
    }
}

#[derive(Debug, Default)]
struct TraceGapReport {
    event_count: usize,
    trace_count: usize,
    suspect_traces: Vec<String>,
}

#[derive(Debug, Default)]
struct TraceStats {
    count: usize,
    event_types: BTreeSet<String>,
}

fn trace_gap_report(journal_root: &Path) -> Result<TraceGapReport, String> {
    if !journal_root.exists() {
        return Ok(TraceGapReport::default());
    }
    if !journal_root.is_dir() {
        return Err(format!(
            "journal root is not a directory: {}",
            journal_root.display()
        ));
    }

    let mut stats: BTreeMap<String, TraceStats> = BTreeMap::new();
    for path in jsonl_files(journal_root)? {
        let file = std::fs::File::open(&path)
            .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        for line in std::io::BufReader::new(file).lines() {
            let line = line.map_err(|err| format!("failed to read {}: {err}", path.display()))?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }
            let value = serde_json::from_str::<serde_json::Value>(trimmed)
                .map_err(|err| format!("invalid JSON in {}: {err}", path.display()))?;
            let Some(trace_id) = value.get("trace_id").and_then(serde_json::Value::as_str) else {
                continue;
            };
            let event_type = value
                .get("event_type")
                .and_then(serde_json::Value::as_str)
                .unwrap_or("unknown");
            let trace_stats = stats.entry(trace_id.to_owned()).or_default();
            trace_stats.count = trace_stats.count.saturating_add(1);
            trace_stats.event_types.insert(event_type.to_owned());
        }
    }

    let event_count = stats.values().map(|trace| trace.count).sum();
    let mut suspect_traces: Vec<String> = stats
        .iter()
        .filter(|(_, trace)| trace.count == 1 && !is_known_single_event_trace(trace))
        .map(|(trace_id, _)| trace_id.clone())
        .take(10)
        .collect();
    suspect_traces.sort();

    Ok(TraceGapReport {
        event_count,
        trace_count: stats.len(),
        suspect_traces,
    })
}

fn jsonl_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    collect_jsonl_files(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_jsonl_files(path: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in std::fs::read_dir(path)
        .map_err(|err| format!("failed to list {}: {err}", path.display()))?
    {
        let entry = entry.map_err(|err| format!("failed to read {}: {err}", path.display()))?;
        let path = entry.path();
        let file_type = entry
            .file_type()
            .map_err(|err| format!("failed to stat {}: {err}", path.display()))?;
        if file_type.is_dir() {
            collect_jsonl_files(&path, files)?;
        } else if file_type.is_file() && path.extension().is_some_and(|ext| ext == "jsonl") {
            files.push(path);
        }
    }
    Ok(())
}

fn is_known_single_event_trace(trace: &TraceStats) -> bool {
    trace.event_types.iter().any(|event_type| {
        matches!(
            event_type.as_str(),
            "clock.unsynced"
                | "harness.version-changed"
                | "skills.changed"
                | "setup.completed"
                | "tempyr.update-candidate"
        )
    })
}

fn check_path_is_native_fs(id: &'static str, label: &str, path: &Path) -> CheckOutcome {
    match path.try_exists() {
        Ok(true) => {}
        Ok(false) => return missing_path_outcome(id, label, path),
        Err(e) => {
            return CheckOutcome::fail(
                id,
                format!("cannot stat {}: {e}", path.display()),
                "Verify the path is accessible to the calling user.",
            );
        }
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
    native_fs_outcome(id, label, &canonical)
}

fn check_path_is_native_fs_as_user(
    id: &'static str,
    label: &str,
    path: &Path,
    user: &str,
) -> CheckOutcome {
    let output = Command::new("sudo")
        .args(["-n", "-u", user, "realpath"])
        .arg(path)
        .output();
    match output {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let canonical = stdout.trim();
            if canonical.is_empty() {
                return CheckOutcome::fail(
                    id,
                    format!(
                        "realpath returned an empty canonical path for {} as {user}",
                        path.display()
                    ),
                    "Verify the runtime user can resolve the path noninteractively.",
                );
            }
            native_fs_outcome(id, label, Path::new(canonical))
        }
        Ok(out) => {
            let detail = output_summary(&out.stdout, &out.stderr);
            if output_summary_indicates_missing_path(&detail) {
                missing_path_outcome(id, label, path)
            } else {
                CheckOutcome::fail(
                    id,
                    format!(
                        "cannot canonicalize {} as {user}{}",
                        path.display(),
                        command_failure_suffix(&out.stdout, &out.stderr)
                    ),
                    "Verify the runtime user can resolve the path noninteractively.",
                )
            }
        }
        Err(e) => CheckOutcome::fail(
            id,
            format!(
                "cannot invoke sudo realpath for {} as {user}: {e}",
                path.display()
            ),
            "Verify sudo is installed and the jam-users sudoers rule is active.",
        ),
    }
}

fn missing_path_outcome(id: &'static str, label: &str, path: &Path) -> CheckOutcome {
    CheckOutcome::warn(
        id,
        format!("{label}: {} does not exist yet", path.display()),
        "Will be created when its owning step runs.",
    )
}

fn native_fs_outcome(id: &'static str, label: &str, canonical: &Path) -> CheckOutcome {
    if is_windows_mount(canonical) {
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

fn check_pinned_binary(
    name: &str,
    path: &Path,
    version_args: &[&str],
    expected_version: &str,
) -> Result<String, String> {
    let metadata = std::fs::metadata(path)
        .map_err(|e| format!("{name} missing at {}: {e}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o111 == 0 {
        return Err(format!("{} is not executable", path.display()));
    }

    let output = Command::new(path)
        .args(version_args)
        .output()
        .map_err(|e| format!("cannot run {}: {e}", path.display()))?;
    if !output.status.success() {
        return Err(format!(
            "{name} version command failed{}",
            command_failure_suffix(&output.stdout, &output.stderr)
        ));
    }
    let Some(actual_version) = parse_first_version(&output.stdout, &output.stderr) else {
        return Err(format!(
            "{name} version output did not include a parseable version"
        ));
    };
    if actual_version == expected_version {
        Ok(actual_version)
    } else {
        Err(format!(
            "{name} version {actual_version} installed, expected {expected_version}"
        ))
    }
}

fn enabled_process_binary_problems(compose_path: &Path, install_dir: &Path) -> Vec<String> {
    let config = match read_process_compose_config(compose_path) {
        Ok(config) => config,
        Err(problem) => return vec![problem],
    };

    config
        .processes
        .iter()
        .filter(|(_, process)| !process.disabled.unwrap_or(false))
        .flat_map(|(name, process)| {
            enabled_process_binary_problems_for_process(name, process, install_dir)
        })
        .collect()
}

fn enabled_ui_static_bundle_problems(compose_path: &Path, ui_static_dir: &Path) -> Vec<String> {
    let config = match read_process_compose_config(compose_path) {
        Ok(config) => config,
        Err(problem) => return vec![problem],
    };
    if !config
        .processes
        .iter()
        .any(|(name, process)| name == "ui-server" && !process.disabled.unwrap_or(false))
    {
        return Vec::new();
    }
    let index = ui_static_dir.join("index.html");
    if index.is_file() {
        Vec::new()
    } else {
        vec![format!(
            "enabled process ui-server: static bundle missing at {}",
            index.display()
        )]
    }
}

fn read_process_compose_config(compose_path: &Path) -> Result<ProcessComposeConfig, String> {
    let content = match std::fs::read_to_string(compose_path) {
        Ok(content) => content,
        Err(err) => {
            return Err(format!(
                "cannot read process-compose config {}: {err}",
                compose_path.display()
            ));
        }
    };
    match serde_yaml::from_str::<ProcessComposeConfig>(&content) {
        Ok(config) => Ok(config),
        Err(err) => Err(format!(
            "cannot parse process-compose config {}: {err}",
            compose_path.display()
        )),
    }
}

fn enabled_process_binary_problems_for_process(
    process_name: &str,
    process: &ProcessComposeProcess,
    install_dir: &Path,
) -> Vec<String> {
    let mut problems = Vec::new();
    let Some(command) = process.command.as_deref() else {
        return vec![format!("enabled process {process_name} has no command")];
    };
    if let Some(problem) = enabled_process_command_binary_problem(
        &format!("enabled process {process_name}"),
        command,
        install_dir,
    ) {
        problems.push(problem);
    }
    if let Some(readiness_command) = process
        .readiness_probe
        .as_ref()
        .and_then(|probe| probe.exec.as_ref())
        .and_then(|exec| exec.command.as_deref())
    {
        if let Some(problem) = enabled_process_command_binary_problem(
            &format!("enabled process {process_name} readiness probe"),
            readiness_command,
            install_dir,
        ) {
            problems.push(problem);
        }
    }
    problems
}

fn enabled_process_command_binary_problem(
    context: &str,
    command: &str,
    install_dir: &Path,
) -> Option<String> {
    let path = first_absolute_command_path(command)?;
    if path.file_name().is_some_and(|name| name == "nats-server") {
        return None;
    }
    if !path.starts_with(install_dir) {
        return None;
    }
    match check_executable_file(&path) {
        Ok(()) => None,
        Err(problem) => Some(format!("{context}: {problem}")),
    }
}

fn first_absolute_command_path(command: &str) -> Option<PathBuf> {
    command
        .split_whitespace()
        .find(|token| token.starts_with('/'))
        .map(|token| PathBuf::from(token.trim_matches('"')))
}

fn check_executable_file(path: &Path) -> Result<(), String> {
    let metadata = std::fs::metadata(path)
        .map_err(|err| format!("binary missing at {}: {err}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    #[cfg(unix)]
    if metadata.permissions().mode() & 0o111 == 0 {
        return Err(format!("{} is not executable", path.display()));
    }
    Ok(())
}

fn parse_first_version(stdout: &[u8], stderr: &[u8]) -> Option<String> {
    let stdout = String::from_utf8_lossy(stdout);
    let stderr = String::from_utf8_lossy(stderr);
    stdout
        .split_whitespace()
        .chain(stderr.split_whitespace())
        .find_map(normalize_version_token)
}

fn normalize_version_token(token: &str) -> Option<String> {
    let trimmed =
        token.trim_matches(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+')));
    let without_prefix = trimmed.strip_prefix('v').unwrap_or(trimmed);
    if without_prefix.contains('.')
        && without_prefix
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_digit())
    {
        Some(without_prefix.to_string())
    } else {
        None
    }
}

fn command_failure_suffix(stdout: &[u8], stderr: &[u8]) -> String {
    let detail = output_summary(stdout, stderr);
    if detail.is_empty() {
        String::new()
    } else {
        format!(": {detail}")
    }
}

fn output_summary(stdout: &[u8], stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr).trim().to_string();
    if !stderr.is_empty() {
        return stderr.lines().next().unwrap_or_default().to_string();
    }
    String::from_utf8_lossy(stdout)
        .trim()
        .lines()
        .next()
        .unwrap_or_default()
        .to_string()
}

fn output_summary_indicates_missing_path(summary: &str) -> bool {
    let lower = summary.to_ascii_lowercase();
    lower.contains("no such file") || lower.contains("not found")
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
    use std::fs;
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
    fn output_summary_missing_path_detection_matches_common_tools() {
        assert!(output_summary_indicates_missing_path(
            "realpath: /home/picker/workers: No such file or directory"
        ));
        assert!(output_summary_indicates_missing_path(
            "command not found: process-compose"
        ));
        assert!(!output_summary_indicates_missing_path(
            "realpath: /home/picker/workers: Permission denied"
        ));
    }

    #[test]
    fn trace_gap_report_warns_on_unexplained_single_entry_trace() {
        let tmp = tempfile::tempdir().unwrap();
        let day = tmp.path().join("2026-05-06");
        fs::create_dir_all(&day).unwrap();
        fs::write(
            day.join("journal.test.jsonl"),
            r#"{"event_type":"task.requested","trace_id":"01ROOT","actor":"jam-cli","payload":{}}
{"event_type":"maestro.session-started","trace_id":"01ROOT","actor":"maestro","payload":{}}
{"event_type":"worktree.created","trace_id":"01SINGLE","actor":"jam-svc-worktree","payload":{}}
"#,
        )
        .unwrap();

        let report = trace_gap_report(tmp.path()).unwrap();

        assert_eq!(report.event_count, 3);
        assert_eq!(report.trace_count, 2);
        assert_eq!(report.suspect_traces, vec!["01SINGLE"]);
    }

    #[test]
    fn trace_gap_report_allows_known_single_entry_traces() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("journal.clock.jsonl"),
            r#"{"event_type":"clock.unsynced","trace_id":"01CLOCK","actor":"jam-clock-watcher","payload":{}}
"#,
        )
        .unwrap();

        let report = trace_gap_report(tmp.path()).unwrap();

        assert!(report.suspect_traces.is_empty());
    }

    #[test]
    fn run_all_checks_returns_27_outcomes() {
        let outcomes = run_all_checks();
        assert_eq!(
            outcomes.len(),
            27,
            "spec §11.4 (13) + security-setup §10 (11) + trace-gap + Phase 9 learned checks (2)"
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

    #[test]
    fn parse_first_version_accepts_known_tool_outputs() {
        assert_eq!(
            parse_first_version(b"nats-server: v2.11.0\n", b""),
            Some("2.11.0".to_string())
        );
        assert_eq!(
            parse_first_version(b"Version: v1.40.1\nCommit: abc\n", b""),
            Some("1.40.1".to_string())
        );
        assert_eq!(
            parse_first_version(b"", b"process-compose version v1.40.1\n"),
            Some("1.40.1".to_string())
        );
    }

    #[test]
    fn parse_first_version_ignores_non_versions() {
        assert_eq!(parse_first_version(b"process-compose\n", b""), None);
        assert_eq!(parse_first_version(b"build 42\n", b""), None);
    }

    #[test]
    fn enabled_process_binary_problems_require_enabled_first_party_binaries() {
        let tmp = tempfile::tempdir().unwrap();
        let install_dir = tmp.path().join("bin");
        fs::create_dir_all(&install_dir).unwrap();
        let compose_path = tmp.path().join("process-compose.yaml");
        fs::write(
            &compose_path,
            format!(
                r#"
version: "0.5"
processes:
  nats:
    command: "{}/nats-server --jetstream"
  jam-nats-bridge:
    command: "{}/jam-nats-bridge"
    disabled: false
  future-service:
    command: "{}/jam-future-service"
    disabled: true
"#,
                install_dir.display(),
                install_dir.display(),
                install_dir.display()
            ),
        )
        .unwrap();

        let problems = enabled_process_binary_problems(&compose_path, &install_dir);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("enabled process jam-nats-bridge"));
        assert!(problems[0].contains("jam-nats-bridge"));
        assert!(!problems[0].contains("jam-future-service"));
    }

    #[test]
    fn enabled_process_binary_problems_accept_executable_enabled_binaries() {
        let tmp = tempfile::tempdir().unwrap();
        let install_dir = tmp.path().join("bin");
        fs::create_dir_all(&install_dir).unwrap();
        let bridge_path = install_dir.join("jam-nats-bridge");
        fs::write(&bridge_path, b"#!/bin/sh\nexit 0\n").unwrap();
        let mut permissions = fs::metadata(&bridge_path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&bridge_path, permissions).unwrap();

        let compose_path = tmp.path().join("process-compose.yaml");
        fs::write(
            &compose_path,
            format!(
                r#"
version: "0.5"
processes:
  jam-nats-bridge:
    command: "{}/jam-nats-bridge"
    disabled: false
"#,
                install_dir.display()
            ),
        )
        .unwrap();

        let problems = enabled_process_binary_problems(&compose_path, &install_dir);
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn enabled_process_binary_problems_require_readiness_probe_binaries() {
        let tmp = tempfile::tempdir().unwrap();
        let install_dir = tmp.path().join("bin");
        fs::create_dir_all(&install_dir).unwrap();
        let service_path = install_dir.join("jam-svc-message");
        fs::write(&service_path, b"#!/bin/sh\nexit 0\n").unwrap();
        let mut permissions = fs::metadata(&service_path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&service_path, permissions).unwrap();

        let compose_path = tmp.path().join("process-compose.yaml");
        fs::write(
            &compose_path,
            format!(
                r#"
version: "0.5"
processes:
  jam-svc-message:
    command: "{}/jam-svc-message"
    readiness_probe:
      exec:
        command: "{}/jam health ping message"
    disabled: false
"#,
                install_dir.display(),
                install_dir.display()
            ),
        )
        .unwrap();

        let problems = enabled_process_binary_problems(&compose_path, &install_dir);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("enabled process jam-svc-message readiness probe"));
        assert!(problems[0].contains("jam"));
    }

    #[test]
    fn enabled_ui_static_bundle_problems_require_index_for_enabled_ui_server() {
        let tmp = tempfile::tempdir().unwrap();
        let compose_path = tmp.path().join("process-compose.yaml");
        fs::write(
            &compose_path,
            r#"
version: "0.5"
processes:
  ui-server:
    command: "/opt/jam/bin/jam-ui-server"
    disabled: false
"#,
        )
        .unwrap();

        let ui_static_dir = tmp.path().join("ui/dist");
        let problems = enabled_ui_static_bundle_problems(&compose_path, &ui_static_dir);
        assert_eq!(problems.len(), 1);
        assert!(problems[0].contains("ui-server"));
        assert!(problems[0].contains("index.html"));

        fs::create_dir_all(&ui_static_dir).unwrap();
        fs::write(ui_static_dir.join("index.html"), "<!doctype html>").unwrap();
        let problems = enabled_ui_static_bundle_problems(&compose_path, &ui_static_dir);
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn enabled_ui_static_bundle_problems_ignore_disabled_ui_server() {
        let tmp = tempfile::tempdir().unwrap();
        let compose_path = tmp.path().join("process-compose.yaml");
        fs::write(
            &compose_path,
            r#"
version: "0.5"
processes:
  ui-server:
    command: "/opt/jam/bin/jam-ui-server"
    disabled: true
"#,
        )
        .unwrap();

        let problems =
            enabled_ui_static_bundle_problems(&compose_path, &tmp.path().join("ui/dist"));
        assert!(problems.is_empty(), "{problems:?}");
    }

    #[test]
    fn nats_authority_parses_common_urls() {
        assert_eq!(
            nats_authority("nats://127.0.0.1:4222").unwrap(),
            "127.0.0.1:4222"
        );
        assert_eq!(
            nats_authority("nats://token@127.0.0.1:4222").unwrap(),
            "127.0.0.1:4222"
        );
        assert_eq!(
            nats_authority("nats://localhost:4222/path").unwrap(),
            "localhost:4222"
        );
        assert!(nats_authority("http://127.0.0.1:4222").is_err());
        assert!(nats_authority("nats://127.0.0.1").is_err());
    }

    #[test]
    fn loopback_authority_recognizes_local_nats_hosts() {
        assert!(is_loopback_authority("127.0.0.1:4222"));
        assert!(is_loopback_authority("localhost:4222"));
        assert!(is_loopback_authority("[::1]:4222"));
        assert!(!is_loopback_authority("192.168.1.2:4222"));
    }
}
